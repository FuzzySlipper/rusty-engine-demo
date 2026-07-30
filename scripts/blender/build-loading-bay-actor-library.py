"""Build the Loading Bay animated actor source library with Blender.

Run from the repository root:

    PYTHONPATH=/usr/lib/python3.14/site-packages blender --background --factory-startup \
      --python scripts/blender/build-loading-bay-actor-library.py -- \
      --source-root /home/stash/mesh-resources/kenney_animated-characters-retro \
      --output-dir content/assets/actor-kit

The Kenney source supplies one rigged model, three animation FBX files, and
several skins. This recipe produces two independently textured GLBs while
retaining shared clip names and normalized world scale. Attack, hit, and death
are small original object-space actions layered on the source rig; gameplay
never derives authority from those clips.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path


def enable_blender_modules() -> None:
    """Expose the distribution NumPy package before enabling Blender add-ons."""

    try:
        import numpy  # noqa: F401
    except ModuleNotFoundError:
        site_packages = Path(
            f"/usr/lib/python{sys.version_info.major}.{sys.version_info.minor}/site-packages"
        )
        if site_packages.is_dir():
            sys.path.insert(0, str(site_packages))
    import addon_utils

    addon_utils.enable("io_scene_fbx")
    addon_utils.enable("io_scene_gltf2")


enable_blender_modules()

import bpy  # noqa: E402


TARGET_HEIGHT = 1.78
FPS = 30
SOURCE_CLIPS = {
    "idle": ("idle.fbx", "Idle"),
    "run": ("run.fbx", "Run"),
    "jump": ("jump.fbx", "Jump"),
}
VARIANTS = (
    ("arc-warden", "zombieMaleA.png", "Arc Warden"),
    ("bay-rusher", "zombieFemaleA.png", "Bay Rusher"),
)
REQUIRED_CLIPS = ("idle", "run", "jump", "attack", "hit", "death")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1 :])


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def reset_scene() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.render.fps = FPS


def import_model(model_path: Path) -> tuple[bpy.types.Object, bpy.types.Object]:
    bpy.ops.import_scene.fbx(filepath=str(model_path))
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if len(armatures) != 1 or len(meshes) != 1:
        raise RuntimeError(
            f"expected one armature and one mesh, got {len(armatures)} and {len(meshes)}"
        )
    armature = armatures[0]
    mesh = meshes[0]
    armature.name = "ActorRig"
    mesh.name = "ActorMesh"

    source_height = max(float(mesh.dimensions.z), 0.000_001)
    scale = TARGET_HEIGHT / source_height
    armature.scale = (scale, scale, scale)
    mesh.scale = (scale, scale, scale)
    for pose_bone in armature.pose.bones:
        pose_bone.custom_shape = None
    return armature, mesh


def install_skin(mesh: bpy.types.Object, texture_path: Path) -> None:
    if len(mesh.data.materials) != 1:
        raise RuntimeError(f"expected one actor material, got {len(mesh.data.materials)}")
    material = mesh.data.materials[0]
    material.name = "actor-skin"
    material.use_nodes = True
    nodes = material.node_tree.nodes
    links = material.node_tree.links
    nodes.clear()
    image = bpy.data.images.load(str(texture_path), check_existing=False)
    image.name = texture_path.stem
    image.colorspace_settings.name = "sRGB"
    output = nodes.new("ShaderNodeOutputMaterial")
    output.name = "Actor Material Output"
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    shader.name = "Actor Skin Shader"
    texture = nodes.new("ShaderNodeTexImage")
    texture.name = "Actor Skin"
    texture.image = image
    texture.interpolation = "Closest"
    links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    links.new(texture.outputs["Alpha"], shader.inputs["Alpha"])
    links.new(shader.outputs["BSDF"], output.inputs["Surface"])
    shader.inputs["Roughness"].default_value = 0.82
    material.surface_render_method = "DITHERED"


def import_source_actions(
    source_root: Path, actor_armature: bpy.types.Object
) -> list[bpy.types.Action]:
    imported_actions: list[bpy.types.Action] = []
    retained_objects = set(bpy.context.scene.objects)
    for clip_id, (filename, source_name) in SOURCE_CLIPS.items():
        actions_before = set(bpy.data.actions)
        bpy.ops.import_scene.fbx(filepath=str(source_root / "Animations" / filename))
        candidates = [
            action
            for action in bpy.data.actions
            if action not in actions_before and action.name.endswith(f"|{source_name}")
        ]
        if len(candidates) != 1:
            raise RuntimeError(
                f"{filename} produced {len(candidates)} `{source_name}` actions"
            )
        action = candidates[0]
        action.name = clip_id
        action.use_fake_user = True
        imported_actions.append(action)

        for obj in list(bpy.context.scene.objects):
            if obj not in retained_objects:
                bpy.data.objects.remove(obj, do_unlink=True)
        for action in list(bpy.data.actions):
            if action not in actions_before and action is not candidates[0]:
                bpy.data.actions.remove(action)

    actor_armature.animation_data_create()
    actor_armature.animation_data.action = imported_actions[0]
    return imported_actions


def key_object_action(
    armature: bpy.types.Object,
    name: str,
    frames: list[tuple[int, tuple[float, float, float], tuple[float, float, float]]],
) -> bpy.types.Action:
    armature.animation_data_create()
    action = bpy.data.actions.new(name=name)
    action.use_fake_user = True
    armature.animation_data.action = action
    armature.rotation_mode = "XYZ"
    for frame, location, rotation in frames:
        armature.location = location
        armature.rotation_euler = rotation
        armature.keyframe_insert(data_path="location", frame=frame, group=name)
        armature.keyframe_insert(data_path="rotation_euler", frame=frame, group=name)
    armature.location = (0.0, 0.0, 0.0)
    armature.rotation_euler = (0.0, 0.0, 0.0)
    return action


def add_original_actions(armature: bpy.types.Object) -> list[bpy.types.Action]:
    return [
        key_object_action(
            armature,
            "attack",
            [
                (0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
                (4, (0.0, -0.09, 0.0), (math.radians(-7), 0.0, 0.0)),
                (10, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
            ],
        ),
        key_object_action(
            armature,
            "hit",
            [
                (0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
                (3, (0.08, 0.04, 0.0), (0.0, math.radians(5), math.radians(11))),
                (8, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
            ],
        ),
        key_object_action(
            armature,
            "death",
            [
                (0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
                (10, (0.0, 0.02, -0.24), (math.radians(52), 0.0, 0.0)),
                (22, (0.0, 0.12, -0.68), (math.radians(88), 0.0, 0.0)),
            ],
        ),
    ]


def install_export_tracks(
    armature: bpy.types.Object, actions: list[bpy.types.Action]
) -> None:
    """Expose every reviewed action as one named glTF animation."""

    armature.animation_data_create()
    armature.animation_data.action = None
    for track in list(armature.animation_data.nla_tracks):
        armature.animation_data.nla_tracks.remove(track)
    for action in actions:
        track = armature.animation_data.nla_tracks.new()
        track.name = action.name
        strip = track.strips.new(action.name, int(action.frame_range[0]), action)
        strip.action_frame_start = action.frame_range[0]
        strip.action_frame_end = action.frame_range[1]


def remove_export_helpers(actor_mesh: bpy.types.Object) -> None:
    """Keep rig-control display meshes out of the shipped actor resource."""

    for obj in list(bpy.data.objects):
        if obj.type == "MESH" and obj is not actor_mesh:
            bpy.data.objects.remove(obj, do_unlink=True)


def validate_export(output_path: Path, skin_filename: str) -> None:
    """Reimport the shipped binary so the manifest cannot overstate its clips."""

    reset_scene()
    bpy.ops.import_scene.gltf(filepath=str(output_path), disable_bone_shape=True)
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if len(armatures) != 1 or len(meshes) != 1:
        raise RuntimeError(
            f"{output_path.name} reimported {len(armatures)} armatures and "
            f"{len(meshes)} meshes"
        )
    imported_clips = tuple(sorted(action.name for action in bpy.data.actions))
    if imported_clips != tuple(sorted(REQUIRED_CLIPS)):
        raise RuntimeError(
            f"{output_path.name} clips are {imported_clips}, "
            f"expected {tuple(sorted(REQUIRED_CLIPS))}"
        )
    height = float(meshes[0].dimensions.z)
    if not math.isclose(height, TARGET_HEIGHT, rel_tol=0.0, abs_tol=0.01):
        raise RuntimeError(
            f"{output_path.name} reimported at height {height}, expected {TARGET_HEIGHT}"
        )
    if len(meshes[0].data.materials) != 1:
        raise RuntimeError(
            f"{output_path.name} reimported with "
            f"{len(meshes[0].data.materials)} materials"
        )
    if skin_filename.removesuffix(".png") not in {image.name for image in bpy.data.images}:
        raise RuntimeError(
            f"{output_path.name} did not retain embedded skin {skin_filename}"
        )


def export_variant(
    source_root: Path,
    output_path: Path,
    skin_filename: str,
) -> dict[str, object]:
    reset_scene()
    armature, mesh = import_model(source_root / "Model" / "characterMedium.fbx")
    install_skin(mesh, source_root / "Skins" / skin_filename)
    source_actions = import_source_actions(source_root, armature)
    authored_actions = add_original_actions(armature)
    actions = source_actions + authored_actions
    if tuple(action.name for action in actions) != REQUIRED_CLIPS:
        raise RuntimeError(
            f"reviewed clip order drifted to {tuple(action.name for action in actions)}"
        )
    clip_records = [
        {
            "id": action.name,
            "frameStart": int(action.frame_range[0]),
            "frameEnd": int(action.frame_range[1]),
            "durationSeconds": round(
                (action.frame_range[1] - action.frame_range[0]) / FPS, 6
            ),
            "origin": "Kenney source"
            if action in source_actions
            else "Loading Bay derivative",
        }
        for action in actions
    ]
    install_export_tracks(armature, actions)
    remove_export_helpers(mesh)

    for obj in bpy.context.scene.objects:
        obj.select_set(obj in {armature, mesh})
    bpy.context.view_layer.objects.active = armature
    output_path.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(output_path),
        export_format="GLB",
        use_selection=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_materials="EXPORT",
        export_image_format="AUTO",
        export_yup=True,
        export_cameras=False,
        export_lights=False,
        export_extras=False,
        export_optimize_animation_size=True,
    )
    validate_export(output_path, skin_filename)
    return {
        "file": output_path.name,
        "sha256": sha256(output_path),
        "bytes": output_path.stat().st_size,
        "skin": skin_filename,
        "clips": clip_records,
        "targetHeight": TARGET_HEIGHT,
        "fps": FPS,
    }


def main() -> None:
    args = parse_args()
    source_root = args.source_root.resolve()
    output_dir = args.output_dir.resolve()
    required = [
        source_root / "Model" / "characterMedium.fbx",
        source_root / "License.txt",
        *[
            source_root / "Animations" / filename
            for filename, _ in SOURCE_CLIPS.values()
        ],
        *[source_root / "Skins" / skin for _, skin, _ in VARIANTS],
    ]
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        raise RuntimeError(f"missing actor source files: {missing}")

    variants = []
    for asset_id, skin, label in VARIANTS:
        record = export_variant(source_root, output_dir / f"{asset_id}.glb", skin)
        record["assetId"] = f"mesh-animation/actor-kit/{asset_id}"
        record["label"] = label
        variants.append(record)

    source_notice = (source_root / "License.txt").read_text(encoding="utf-8-sig")
    normalized_notice = "\n".join(
        line.strip(" \t\r").replace("\t", "")
        for line in source_notice.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    ).strip() + "\n"
    notice_path = output_dir / "KENNEY-CC0-LICENSE.txt"
    notice_path.write_text(normalized_notice, encoding="utf-8")
    manifest = {
        "schemaVersion": 1,
        "generator": "scripts/blender/build-loading-bay-actor-library.py",
        "blenderVersion": bpy.app.version_string,
        "source": {
            "pack": "Kenney Animated Characters Retro",
            "author": "Kenney",
            "url": "https://kenney.nl/assets/animated-characters",
            "license": "CC0 1.0",
            "files": [
                {
                    "path": str(path.relative_to(source_root)),
                    "sha256": sha256(path),
                }
                for path in required
            ],
        },
        "licenseNotice": {
            "path": "KENNEY-CC0-LICENSE.txt",
            "normalization": "UTF-8 LF with source indentation and trailing whitespace removed",
            "sha256": sha256(notice_path),
            "bytes": notice_path.stat().st_size,
        },
        "variants": variants,
    }
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "source-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
