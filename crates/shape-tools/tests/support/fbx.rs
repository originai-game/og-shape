//! Test support shared with the golden cases: a binary FBX 7.4 written
//! by hand, since no FBX writer exists here.

/// The format's records, properties and arrays (raw and zlib), enough for
/// a mesh, its model tree and a texture.
mod fbx_writer {
    pub enum P {
        L(i64),
        I(i32),
        D(f64),
        S(String),
        R(Vec<u8>),
        Doubles(Vec<f64>, bool),
        Ints(Vec<i32>, bool),
    }
    pub struct N {
        pub name: &'static str,
        pub props: Vec<P>,
        pub children: Vec<N>,
    }
    pub fn n(name: &'static str, props: Vec<P>, children: Vec<N>) -> N {
        N {
            name,
            props,
            children,
        }
    }
    pub fn s(v: &str) -> P {
        P::S(v.to_string())
    }
    /// `P: name, type, label, flags, values…` in a Properties70.
    pub fn p70(entries: Vec<(&str, Vec<P>)>) -> N {
        n(
            "Properties70",
            vec![],
            entries
                .into_iter()
                .map(|(k, vals)| {
                    let mut props = vec![s(k), s(""), s(""), s("")];
                    props.extend(vals);
                    n("P", props, vec![])
                })
                .collect(),
        )
    }
    fn array(out: &mut Vec<u8>, kind: u8, count: usize, raw: Vec<u8>, zip: bool) {
        out.push(kind);
        out.extend((count as u32).to_le_bytes());
        let data = if zip {
            miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6)
        } else {
            raw
        };
        out.extend((u32::from(zip)).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
    }
    fn prop(out: &mut Vec<u8>, p: &P) {
        match p {
            P::L(v) => {
                out.push(b'L');
                out.extend(v.to_le_bytes());
            }
            P::I(v) => {
                out.push(b'I');
                out.extend(v.to_le_bytes());
            }
            P::D(v) => {
                out.push(b'D');
                out.extend(v.to_le_bytes());
            }
            P::S(v) => {
                out.push(b'S');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(v.as_bytes());
            }
            P::R(v) => {
                out.push(b'R');
                out.extend((v.len() as u32).to_le_bytes());
                out.extend(v);
            }
            P::Doubles(v, zip) => {
                let raw = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                array(out, b'd', v.len(), raw, *zip);
            }
            P::Ints(v, zip) => {
                let raw = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                array(out, b'i', v.len(), raw, *zip);
            }
        }
    }
    fn node(out: &mut Vec<u8>, node: &N) {
        let start = out.len();
        out.extend([0u8; 12]);
        out.push(node.name.len() as u8);
        out.extend(node.name.as_bytes());
        let props_start = out.len();
        for p in &node.props {
            prop(out, p);
        }
        let props_len = out.len() - props_start;
        for c in &node.children {
            self::node(out, c);
        }
        if !node.children.is_empty() {
            out.extend([0u8; 13]);
        }
        let end = out.len() as u32;
        out[start..start + 4].copy_from_slice(&end.to_le_bytes());
        out[start + 4..start + 8].copy_from_slice(&(node.props.len() as u32).to_le_bytes());
        out[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
    }
    pub fn file(nodes: &[N]) -> Vec<u8> {
        let mut out = b"Kaydara FBX Binary  \0\x1a\0".to_vec();
        out.extend(7400u32.to_le_bytes());
        for n in nodes {
            node(&mut out, n);
        }
        out.extend([0u8; 13]);
        out
    }
}

/// A unit cube (cm) as six quads with UVs and two materials, under a
/// moved and turned parent, with a rotation pivot, an Euler order and an
/// embedded PNG.
pub fn binary_fbx() -> Vec<u8> {
    use fbx_writer::{P, file, n, p70, s};
    let verts: Vec<f64> = (0..8)
        .flat_map(|i| [(i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64])
        .collect();
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let index: Vec<i32> = quads
        .iter()
        .flat_map(|q| [q[0], q[1], q[2], -q[3] - 1])
        .collect();
    let uv = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
    let uv_index: Vec<i32> = (0..24).map(|k| k % 4).collect();
    let png = shape_tools::png(&shape_tools::Image {
        width: 16,
        height: 8,
        rgba: vec![200; 16 * 8 * 4],
    });
    file(&[
        n(
            "FBXHeaderExtension",
            vec![],
            vec![n("FBXVersion", vec![P::I(7400)], vec![])],
        ),
        n(
            "GlobalSettings",
            vec![],
            vec![p70(vec![
                ("UpAxis", vec![P::I(1)]),
                ("FrontAxis", vec![P::I(2)]),
                ("CoordAxis", vec![P::I(0)]),
                ("UnitScaleFactor", vec![P::D(1.0)]),
            ])],
        ),
        n(
            "Objects",
            vec![],
            vec![
                n(
                    "Geometry",
                    vec![P::L(100), s("Cube\0\u{1}Geometry"), s("Mesh")],
                    vec![
                        n("Vertices", vec![P::Doubles(verts, true)], vec![]),
                        n("PolygonVertexIndex", vec![P::Ints(index, false)], vec![]),
                        n(
                            "LayerElementUV",
                            vec![P::I(0)],
                            vec![
                                n("MappingInformationType", vec![s("ByPolygonVertex")], vec![]),
                                n("ReferenceInformationType", vec![s("IndexToDirect")], vec![]),
                                n("UV", vec![P::Doubles(uv, true)], vec![]),
                                n("UVIndex", vec![P::Ints(uv_index, true)], vec![]),
                            ],
                        ),
                        n(
                            "LayerElementMaterial",
                            vec![P::I(0)],
                            vec![
                                n("MappingInformationType", vec![s("ByPolygon")], vec![]),
                                n(
                                    "Materials",
                                    vec![P::Ints(vec![0, 0, 1, 1, 1, 1], false)],
                                    vec![],
                                ),
                            ],
                        ),
                    ],
                ),
                n(
                    "Model",
                    vec![P::L(200), s("Base\0\u{1}Model"), s("Null")],
                    vec![p70(vec![(
                        "Lcl Translation",
                        vec![P::D(10.0), P::D(0.0), P::D(0.0)],
                    )])],
                ),
                n(
                    "Model",
                    vec![P::L(300), s("Cube\0\u{1}Model"), s("Mesh")],
                    vec![p70(vec![
                        ("Lcl Rotation", vec![P::D(0.0), P::D(90.0), P::D(0.0)]),
                        ("RotationPivot", vec![P::D(1.0), P::D(0.0), P::D(0.0)]),
                        ("RotationOrder", vec![P::I(4)]),
                        ("Lcl Scaling", vec![P::D(2.0), P::D(2.0), P::D(2.0)]),
                    ])],
                ),
                n(
                    "Material",
                    vec![P::L(400), s("Steel\0\u{1}Material"), s("")],
                    vec![],
                ),
                n(
                    "Material",
                    vec![P::L(401), s("Paint\0\u{1}Material"), s("")],
                    vec![],
                ),
                n(
                    "Video",
                    vec![P::L(500), s("Tex\0\u{1}Video"), s("Clip")],
                    vec![
                        n("RelativeFilename", vec![s("textures\\albedo.png")], vec![]),
                        n("Content", vec![P::R(png)], vec![]),
                    ],
                ),
            ],
        ),
        n(
            "Connections",
            vec![],
            vec![
                n("C", vec![s("OO"), P::L(200), P::L(0)], vec![]),
                n("C", vec![s("OO"), P::L(300), P::L(200)], vec![]),
                n("C", vec![s("OO"), P::L(100), P::L(300)], vec![]),
                n("C", vec![s("OO"), P::L(400), P::L(300)], vec![]),
                n("C", vec![s("OO"), P::L(401), P::L(300)], vec![]),
            ],
        ),
    ])
}
