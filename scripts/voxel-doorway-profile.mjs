function invariant(condition, message) {
  if (!condition) throw new Error(`voxel doorway profile failed: ${message}`);
}

export function doorwayInteriorProfile(object) {
  const runs = object.defaultFrame?.representation?.sparseRuns;
  invariant(Array.isArray(runs), `${object.assetId} must use sparse runs`);
  const [minimumX, minimumY] = object.bounds.min;
  const maximumX = object.bounds.max[0];
  const rows = new Map();
  for (const run of runs) {
    const [startX, y] = run.start;
    let occupied = rows.get(y);
    if (occupied === undefined) {
      occupied = new Set();
      rows.set(y, occupied);
    }
    for (let offset = 0; offset < run.length; offset += 1) {
      occupied.add(startX + offset);
    }
  }
  const fullWidth = maximumX - minimumX + 1;
  const headerStart = [...rows.entries()]
    .filter(([y, occupied]) => y >= minimumY && occupied.size === fullWidth)
    .map(([y]) => y)
    .sort((left, right) => left - right)[0];
  invariant(
    headerStart !== undefined,
    `${object.assetId} has no full-width header`,
  );

  const walkHeightCells = new Set();
  for (const [y, occupied] of rows) {
    if (y >= headerStart) continue;
    for (const x of occupied) walkHeightCells.add(x);
  }
  const ordered = [...walkHeightCells].sort((left, right) => left - right);
  let largestGap;
  for (let index = 1; index < ordered.length; index += 1) {
    const previous = ordered[index - 1];
    const current = ordered[index];
    if (
      current > previous + 1 &&
      (largestGap === undefined || current - previous > largestGap.width)
    ) {
      largestGap = {
        leftCell: previous,
        rightCell: current,
        width: current - previous,
      };
    }
  }
  invariant(
    largestGap !== undefined,
    `${object.assetId} has no walk-height opening`,
  );
  const { cellSize, pivot } = object.grid;
  return Object.freeze({
    headerStartCell: headerStart,
    leftOccupiedCell: largestGap.leftCell,
    rightOccupiedCell: largestGap.rightCell,
    localOpeningMin: (largestGap.leftCell + 1 - pivot[0]) * cellSize,
    localOpeningMax: (largestGap.rightCell - pivot[0]) * cellSize,
  });
}
