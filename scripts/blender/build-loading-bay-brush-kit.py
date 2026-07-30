"""Build the original Loading Bay voxel-brush source meshes.

Run from the repository root:

    blender --background --python scripts/blender/build-loading-bay-brush-kit.py

The generated GLB and mesh JSON files are source inputs. Studio owns conversion
into canonical voxel-object assets and placement into the project.
"""

from __future__ import annotations

import json
import struct
from pathlib import Path

import bpy


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "content" / "assets" / "brush-kit"
MATERIAL_COLOR = [0.31, 0.38, 0.43, 1.0]


def add_box(vertices, indices, center, size):
    cx, cy, cz = center
    sx, sy, sz = (value / 2.0 for value in size)
    base = len(vertices) // 3
    corners = [
        (cx - sx, cy - sy, cz - sz),
        (cx + sx, cy - sy, cz - sz),
        (cx + sx, cy + sy, cz - sz),
        (cx - sx, cy + sy, cz - sz),
        (cx - sx, cy - sy, cz + sz),
        (cx + sx, cy - sy, cz + sz),
        (cx + sx, cy + sy, cz + sz),
        (cx - sx, cy + sy, cz + sz),
    ]
    for corner in corners:
        vertices.extend(corner)
    for face in [
        (0, 3, 2, 1),
        (4, 5, 6, 7),
        (0, 1, 5, 4),
        (3, 7, 6, 2),
        (0, 4, 7, 3),
        (1, 2, 6, 5),
    ]:
        a, b, c, d = (base + offset for offset in face)
        indices.extend((a, b, c, a, c, d))


def wall_conservative():
    return [
        ((0.0, 1.0, 0.0625), (2.0, 2.0, 0.125)),
        ((0.0, 0.125, 0.125), (2.0, 0.25, 0.25)),
        ((0.0, 1.875, 0.125), (2.0, 0.25, 0.25)),
        ((0.0, 1.0, 0.105), (1.75, 0.125, 0.21)),
        ((-0.75, 1.0, 0.115), (0.125, 1.5, 0.23)),
        ((0.75, 1.0, 0.115), (0.125, 1.5, 0.23)),
    ]


def wall_dense():
    boxes = [
        ((0.0, 1.0, 0.0625), (2.0, 2.0, 0.125)),
        ((0.0, 0.09375, 0.125), (2.0, 0.1875, 0.25)),
        ((0.0, 1.90625, 0.125), (2.0, 0.1875, 0.25)),
        ((-0.875, 1.0, 0.125), (0.125, 1.625, 0.25)),
        ((0.875, 1.0, 0.125), (0.125, 1.625, 0.25)),
    ]
    for y in (0.375, 0.625, 0.875, 1.125, 1.375, 1.625):
        boxes.append(((0.0, y, 0.109375), (1.5, 0.0625, 0.21875)))
    for x in (-0.5625, -0.1875, 0.1875, 0.5625):
        boxes.append(((x, 1.0, 0.140625), (0.0625, 1.25, 0.21875)))
    for x in (-0.6875, 0.0, 0.6875):
        boxes.append(((x, 1.0, 0.171875), (0.1875, 0.375, 0.15625)))
    return boxes


def corner():
    return [
        ((0.0, 1.0, 0.9375), (2.0, 2.0, 0.125)),
        ((-0.9375, 1.0, 0.0), (0.125, 2.0, 2.0)),
        ((0.0, 0.125, 0.875), (2.0, 0.25, 0.25)),
        ((-0.875, 0.125, 0.0), (0.25, 0.25, 2.0)),
        ((-0.875, 1.0, 0.875), (0.25, 1.5, 0.25)),
    ]


def doorway():
    return [
        ((-1.25, 1.25, 0.0), (0.5, 2.5, 0.25)),
        ((1.25, 1.25, 0.0), (0.5, 2.5, 0.25)),
        ((0.0, 2.25, 0.0), (2.0, 0.5, 0.25)),
        ((-0.875, 1.25, 0.125), (0.125, 1.5, 0.25)),
        ((0.875, 1.25, 0.125), (0.125, 1.5, 0.25)),
        ((0.0, 2.0, 0.125), (1.625, 0.125, 0.25)),
    ]


def vent():
    boxes = [
        ((0.0, 1.0, 0.0), (2.0, 2.0, 0.125)),
        ((0.0, 0.25, 0.125), (1.75, 0.25, 0.25)),
        ((0.0, 1.75, 0.125), (1.75, 0.25, 0.25)),
        ((-0.75, 1.0, 0.125), (0.25, 1.25, 0.25)),
        ((0.75, 1.0, 0.125), (0.25, 1.25, 0.25)),
    ]
    for y in (0.625, 0.875, 1.125, 1.375):
        boxes.append(((0.0, y, 0.1875), (1.25, 0.125, 0.375)))
    return boxes


def column():
    return [
        ((0.0, 1.0, 0.0), (0.5, 2.0, 0.5)),
        ((0.0, 0.125, 0.0), (0.75, 0.25, 0.75)),
        ((0.0, 1.875, 0.0), (0.75, 0.25, 0.75)),
        ((0.0, 1.0, 0.3125), (0.25, 1.5, 0.125)),
        ((0.3125, 1.0, 0.0), (0.125, 1.5, 0.25)),
    ]


def floor_strip():
    boxes = [
        ((0.0, 0.0625, 0.0), (2.0, 0.125, 2.0)),
        ((-0.75, 0.125, 0.0), (0.25, 0.25, 2.0)),
        ((0.75, 0.125, 0.0), (0.25, 0.25, 2.0)),
    ]
    for z in (-0.75, -0.25, 0.25, 0.75):
        boxes.append(((0.0, 0.109375, z), (1.25, 0.09375, 0.125)))
    return boxes


def ceiling_strip():
    return [
        ((0.0, 0.0625, 0.0), (2.0, 0.125, 2.0)),
        ((0.0, 0.1875, -0.75), (2.0, 0.25, 0.25)),
        ((0.0, 0.1875, 0.75), (2.0, 0.25, 0.25)),
        ((-0.75, 0.25, 0.0), (0.25, 0.375, 1.25)),
        ((0.75, 0.25, 0.0), (0.25, 0.375, 1.25)),
    ]


def landmark():
    boxes = [
        ((0.0, 0.125, 0.0), (2.0, 0.25, 1.0)),
        ((0.0, 1.875, 0.0), (2.0, 0.25, 1.0)),
        ((-0.875, 1.0, 0.0), (0.25, 1.5, 1.0)),
        ((0.875, 1.0, 0.0), (0.25, 1.5, 1.0)),
        ((0.0, 1.0, -0.4375), (1.5, 1.5, 0.125)),
    ]
    for y in (0.5, 0.75, 1.0, 1.25, 1.5):
        boxes.append(((0.0, y, 0.0), (1.125, 0.125, 0.75)))
    for x in (-0.5, 0.0, 0.5):
        boxes.append(((x, 1.0, 0.4375), (0.125, 1.25, 0.125)))
    return boxes


MODULES = {
    "wall-conservative": wall_conservative(),
    "wall-dense": wall_dense(),
    "corner": corner(),
    "doorway": doorway(),
    "vent-panel": vent(),
    "column": column(),
    "floor-strip": floor_strip(),
    "ceiling-strip": ceiling_strip(),
    "landmark-relay": landmark(),
}


def write_mesh_json(name, positions, indices):
    payload = {
        "schemaVersion": 1,
        "name": name,
        "positions": positions,
        "normals": [component for _ in range(len(positions) // 3) for component in (0, 1, 0)],
        "indices": indices,
        "materials": [
            {
                "slot": 0,
                "name": name,
                "color": MATERIAL_COLOR,
                "texture": None,
            }
        ],
        "groups": [{"materialSlot": 0, "start": 0, "count": len(indices)}],
        "collision": "visualOnly",
    }
    path = OUTPUT / f"{name}.mesh.json"
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def write_glb(name, positions, indices):
    mesh = bpy.data.meshes.new(name)
    vertices = [
        tuple(positions[offset : offset + 3])
        for offset in range(0, len(positions), 3)
    ]
    triangles = [
        tuple(indices[offset : offset + 3])
        for offset in range(0, len(indices), 3)
    ]
    mesh.from_pydata(vertices, [], triangles)
    mesh.update()
    material = bpy.data.materials.new(name)
    material.diffuse_color = MATERIAL_COLOR
    mesh.materials.append(material)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)

    # Keep the recipe runnable in minimal headless Blender packages whose glTF
    # add-on may not have its optional NumPy dependency. The mesh is still
    # constructed through Blender above; this compact writer serializes the
    # same positions and indices as one standards-compliant GLB primitive.
    position_bytes = struct.pack(f"<{len(positions)}f", *positions)
    index_bytes = struct.pack(f"<{len(indices)}I", *indices)
    binary = position_bytes + index_bytes
    while len(binary) % 4:
        binary += b"\0"
    points = [
        positions[offset : offset + 3]
        for offset in range(0, len(positions), 3)
    ]
    document = {
        "asset": {"version": "2.0", "generator": "Loading Bay brush kit Blender recipe"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": name}],
        "meshes": [
            {
                "name": name,
                "primitives": [
                    {
                        "attributes": {"POSITION": 0},
                        "indices": 1,
                        "material": 0,
                        "mode": 4,
                    }
                ],
            }
        ],
        "materials": [
            {
                "name": name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": MATERIAL_COLOR,
                    "metallicFactor": 0.15,
                    "roughnessFactor": 0.78,
                },
            }
        ],
        "buffers": [{"byteLength": len(binary)}],
        "bufferViews": [
            {
                "buffer": 0,
                "byteOffset": 0,
                "byteLength": len(position_bytes),
                "target": 34962,
            },
            {
                "buffer": 0,
                "byteOffset": len(position_bytes),
                "byteLength": len(index_bytes),
                "target": 34963,
            },
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": len(points),
                "type": "VEC3",
                "min": [min(point[axis] for point in points) for axis in range(3)],
                "max": [max(point[axis] for point in points) for axis in range(3)],
            },
            {
                "bufferView": 1,
                "componentType": 5125,
                "count": len(indices),
                "type": "SCALAR",
                "min": [min(indices)],
                "max": [max(indices)],
            },
        ],
    }
    json_bytes = json.dumps(document, separators=(",", ":")).encode("utf-8")
    while len(json_bytes) % 4:
        json_bytes += b" "
    total = 12 + 8 + len(json_bytes) + 8 + len(binary)
    glb = (
        struct.pack("<III", 0x46546C67, 2, total)
        + struct.pack("<II", len(json_bytes), 0x4E4F534A)
        + json_bytes
        + struct.pack("<II", len(binary), 0x004E4942)
        + binary
    )
    (OUTPUT / f"{name}.glb").write_bytes(glb)


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    manifest = []
    for name, boxes in MODULES.items():
        positions = []
        indices = []
        for center, size in boxes:
            add_box(positions, indices, center, size)
        minima = [
            min(positions[axis::3])
            for axis in range(3)
        ]
        positions = [
            value - minima[index % 3]
            for index, value in enumerate(positions)
        ]
        write_mesh_json(name, positions, indices)
        write_glb(name, positions, indices)
        manifest.append(
            {
                "name": name,
                "sourceBoxes": len(boxes),
                "sourceVertices": len(positions) // 3,
                "sourceTriangles": len(indices) // 3,
            }
        )
    (OUTPUT / "source-manifest.json").write_text(
        json.dumps({"schemaVersion": 1, "modules": manifest}, indent=2) + "\n",
        encoding="utf-8",
    )


main()
