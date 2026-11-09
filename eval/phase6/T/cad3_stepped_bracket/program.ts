export default function (og) {
  const s = og.model({ name: "stepped_bracket" });
  const thickness = 30; // extrusion depth along z, centred on z = 0
  const b = bracket(thickness);
  return s;

  // The profile lies in the world XY plane (as in the source), extruded along z.
  function bracket(depth,
    xLeftUpper = -48.31, yLower = 15.03, upperWidth = 32, rightExt = 37,
    stepBack = 30, upperExt = 46, lowerHeight = 11, lowerFlange = 7,
    upperHeight = 42, middleHeight = 35) {
    const xInner = xLeftUpper + upperWidth;
    const xRight = xInner + rightExt;
    const xStep = xRight - stepBack;
    const xFarLeft = xStep - upperExt;
    const yMiddleTop = yLower + middleHeight;
    const yUpper = yLower + upperHeight;
    const yLowerInner = yLower - lowerHeight;
    const yLowerTop = yLower - lowerFlange;
    const profile = og.poly({ points: [
      [xLeftUpper, yLower], [xLeftUpper, yMiddleTop], [xInner, yMiddleTop],
      [xInner, yLowerInner], [xRight, yLowerInner], [xRight, yLowerTop],
      [xStep, yLowerTop], [xStep, yUpper], [xFarLeft, yUpper], [xFarLeft, yLower],
    ] });
    const p = s.extrude({ name: "bracket", profile, height: depth, bothSides: true });
    const up = [0, 1, 0];
    s.hole(p, { diameter: 7, at: [og.frame({ origin: [xRight - 15, yLowerTop, 0], normal: up })] });
    s.hole(p, { diameter: 4.5, countersink: [8, og.deg(90)],
      at: [og.frame({ origin: [xLeftUpper + 16, yUpper, 0], normal: up })] });
    s.material(p, { color: "#a0a0a0", metalness: 0.6, roughness: 0.4 });
    return p;
  }
}
