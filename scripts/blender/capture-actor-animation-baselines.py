"""Render independent Blender baselines for the committed actor GLBs and source FBXs.

This deliberately does not import or call Rusty Engine or Three.  It renders the
real admitted GLB bytes and the original Kenney model/animation FBXs at identical
normalized clip times so an exported-source defect cannot certify itself.

Run from the repository root:

    blender --background --factory-startup \
      --python scripts/blender/capture-actor-animation-baselines.py -- \
      --source-root /home/stash/mesh-resources/kenney_animated-characters-retro \
      --output-dir /tmp/loading-bay-actor-blender-baseline
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path


def enable_blender_modules() -> None:
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
from mathutils import Matrix, Vector  # noqa: E402


TARGET_HEIGHT = 1.78
TIMES = (0.0, 0.25, 0.5, 0.75, 1.0)
SOURCE_CLIPS = {
    "idle": ("idle.fbx", "Idle"),
    "run": ("run.fbx", "Run"),
    "jump": ("jump.fbx", "Jump"),
}
ACTORS = ("bay-rusher", "arc-warden")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1 :])


def reset_scene() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "MATERIAL"
    scene.display.shading.show_shadows = True
    scene.display.shading.show_cavity = True
    scene.render.resolution_x = 360
    scene.render.resolution_y = 360
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.film_transparent = False
    scene.render.fps = 30
    scene.world = bpy.data.worlds.new("BaselineWorld")
    scene.world.color = (0.035, 0.045, 0.06)


def visible_mesh_bounds() -> tuple[Vector, Vector]:
    points = [
        obj.matrix_world @ Vector(corner)
        for obj in bpy.context.scene.objects
        if obj.type == "MESH"
        for corner in obj.bound_box
    ]
    if not points:
        raise RuntimeError("baseline scene has no visible mesh bounds")
    return (
        Vector(tuple(min(point[axis] for point in points) for axis in range(3))),
        Vector(tuple(max(point[axis] for point in points) for axis in range(3))),
    )


def add_camera_and_lights() -> None:
    bounds_min, bounds_max = visible_mesh_bounds()
    center = (bounds_min + bounds_max) * 0.5
    extent = bounds_max - bounds_min
    framing = max(extent.x, extent.y, extent.z, 0.1)
    camera_data = bpy.data.cameras.new("BaselineCamera")
    camera = bpy.data.objects.new("BaselineCamera", camera_data)
    bpy.context.collection.objects.link(camera)
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = framing * 1.35
    camera.location = center + Vector((framing * 1.45, -framing * 2.3, framing * 0.7))
    point_camera(camera, center)
    bpy.context.scene.camera = camera

    key_data = bpy.data.lights.new("BaselineKey", "AREA")
    key_data.energy = 900
    key_data.shape = "DISK"
    key_data.size = 4.0
    key = bpy.data.objects.new("BaselineKey", key_data)
    bpy.context.collection.objects.link(key)
    key.location = center + Vector((-framing * 1.7, -framing * 2.2, framing * 2.5))
    point_camera(key, center)

    fill_data = bpy.data.lights.new("BaselineFill", "AREA")
    fill_data.energy = 500
    fill_data.size = 3.0
    fill = bpy.data.objects.new("BaselineFill", fill_data)
    bpy.context.collection.objects.link(fill)
    fill.location = center + Vector((framing * 2.2, framing * 0.5, framing * 1.4))
    point_camera(fill, center)

    plane_data = bpy.data.meshes.new("BaselineFloor")
    plane = bpy.data.objects.new("BaselineFloor", plane_data)
    bpy.context.collection.objects.link(plane)
    plane_data.from_pydata(
        [
            (center.x - framing * 2.0, center.y - framing * 2.0, bounds_min.z),
            (center.x + framing * 2.0, center.y - framing * 2.0, bounds_min.z),
            (center.x + framing * 2.0, center.y + framing * 2.0, bounds_min.z),
            (center.x - framing * 2.0, center.y + framing * 2.0, bounds_min.z),
        ],
        [],
        [(0, 1, 2, 3)],
    )
    material = bpy.data.materials.new("BaselineFloorMaterial")
    material.diffuse_color = (0.12, 0.14, 0.18, 1.0)
    plane.data.materials.append(material)


def point_camera(obj: bpy.types.Object, target: Vector) -> None:
    obj.rotation_euler = (target - obj.location).to_track_quat("-Z", "Y").to_euler()


def import_committed_glb(path: Path) -> tuple[bpy.types.Object, dict[str, bpy.types.Action]]:
    bpy.ops.import_scene.gltf(filepath=str(path), disable_bone_shape=True)
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    if len(armatures) != 1:
        raise RuntimeError(f"{path.name}: expected one armature, got {len(armatures)}")
    actions = {action.name: action for action in bpy.data.actions}
    return armatures[0], actions


def import_original_source(
    source_root: Path,
) -> tuple[bpy.types.Object, dict[str, bpy.types.Action]]:
    bpy.ops.import_scene.fbx(filepath=str(source_root / "Model" / "characterMedium.fbx"))
    armatures = [obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE"]
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if len(armatures) != 1 or len(meshes) != 1:
        raise RuntimeError("source model must import as one armature and one mesh")
    armature = armatures[0]
    mesh = meshes[0]
    source_height = max(float(mesh.dimensions.z), 0.000_001)
    scale = TARGET_HEIGHT / source_height
    armature.scale = (scale, scale, scale)
    bpy.context.view_layer.update()

    retained_objects = set(bpy.context.scene.objects)
    actions: dict[str, bpy.types.Action] = {}
    for clip_id, (filename, source_name) in SOURCE_CLIPS.items():
        actions_before = set(bpy.data.actions)
        bpy.ops.import_scene.fbx(filepath=str(source_root / "Animations" / filename))
        candidates = [
            action
            for action in bpy.data.actions
            if action not in actions_before and action.name.endswith(f"|{source_name}")
        ]
        if len(candidates) != 1:
            raise RuntimeError(f"{filename}: expected one {source_name} action")
        source_armatures = [
            obj
            for obj in bpy.context.scene.objects
            if obj not in retained_objects and obj.type == "ARMATURE"
        ]
        if len(source_armatures) != 1:
            raise RuntimeError(
                f"{filename}: expected one source armature, got {len(source_armatures)}"
            )
        action = retarget_source_action(
            source_armatures[0], candidates[0], armature, clip_id
        )
        actions[clip_id] = action
        for obj in list(bpy.context.scene.objects):
            if obj not in retained_objects:
                bpy.data.objects.remove(obj, do_unlink=True)
        for candidate in list(bpy.data.actions):
            if candidate not in actions_before and candidate is not action:
                bpy.data.actions.remove(candidate)
    return armature, actions


def retarget_source_action(
    source_armature: bpy.types.Object,
    source_action: bpy.types.Action,
    target_armature: bpy.types.Object,
    clip_id: str,
) -> bpy.types.Action:
    """Bake portable source rotations onto the model FBX bind skeleton.

    The animation-only Kenney FBXs have the same named hierarchy but a distinct
    rest pose.  Applying their keyed joint locations directly to the model bind
    skeleton is the exact source of the historic stretched limbs.  This
    independent baseline keeps model bone lengths and copies only each source
    pose's local rotation plus true root motion.
    """

    source_names = set(source_armature.pose.bones.keys())
    target_names = set(target_armature.pose.bones.keys())
    if source_names != target_names:
        raise RuntimeError(
            f"{clip_id}: source/model bone inventories differ: "
            f"source-only={sorted(source_names - target_names)}, "
            f"model-only={sorted(target_names - source_names)}"
        )
    source_armature.animation_data_create()
    source_armature.animation_data.action = source_action
    if len(source_action.slots) == 1:
        source_armature.animation_data.action_slot = source_action.slots[0]
    target_armature.animation_data_create()
    baked = bpy.data.actions.new(name=clip_id)
    baked.use_fake_user = True
    target_armature.animation_data.action = baked
    start, end = (int(round(value)) for value in source_action.frame_range)
    for frame in range(start, end + 1):
        bpy.context.scene.frame_set(frame)
        bpy.context.view_layer.update()
        for target_pose in target_armature.pose.bones:
            source_pose = source_armature.pose.bones[target_pose.name]
            source_location, source_rotation, _ = source_pose.matrix_basis.decompose()
            target_pose.location = (
                source_location if target_pose.parent is None else Vector((0, 0, 0))
            )
            target_pose.rotation_mode = "QUATERNION"
            target_pose.rotation_quaternion = source_rotation
            target_pose.scale = (1.0, 1.0, 1.0)
            target_pose.keyframe_insert(
                data_path="location", frame=frame, group=target_pose.name
            )
            target_pose.keyframe_insert(
                data_path="rotation_quaternion", frame=frame, group=target_pose.name
            )
            target_pose.keyframe_insert(
                data_path="scale", frame=frame, group=target_pose.name
            )
    reset_armature_pose(target_armature)
    return baked


def reset_armature_pose(armature: bpy.types.Object) -> None:
    armature.animation_data_create()
    armature.animation_data.action = None
    for track in armature.animation_data.nla_tracks:
        track.mute = True
    for pose_bone in armature.pose.bones:
        pose_bone.matrix_basis = Matrix.Identity(4)
    armature.data.pose_position = "REST"
    bpy.context.view_layer.update()
    armature.data.pose_position = "POSE"
    bpy.context.view_layer.update()


def evaluated_mesh_bounds() -> tuple[Vector, Vector, int]:
    dependency_graph = bpy.context.evaluated_depsgraph_get()
    points: list[Vector] = []
    vertex_count = 0
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH" or obj.name == "BaselineFloor":
            continue
        evaluated = obj.evaluated_get(dependency_graph)
        mesh = evaluated.to_mesh()
        try:
            points.extend(evaluated.matrix_world @ vertex.co for vertex in mesh.vertices)
            vertex_count += len(mesh.vertices)
        finally:
            evaluated.to_mesh_clear()
    if not points:
        raise RuntimeError("baseline sample has no evaluated mesh vertices")
    return (
        Vector(tuple(min(point[axis] for point in points) for axis in range(3))),
        Vector(tuple(max(point[axis] for point in points) for axis in range(3))),
        vertex_count,
    )


def render_samples(
    output_dir: Path,
    baseline: str,
    actor: str,
    armature: bpy.types.Object,
    actions: dict[str, bpy.types.Action],
) -> list[dict[str, object]]:
    armature.animation_data_create()
    for track in armature.animation_data.nla_tracks:
        track.mute = True
    records: list[dict[str, object]] = []
    for clip, action in sorted(actions.items()):
        armature.animation_data.action = action
        _, end = (float(value) for value in action.frame_range)
        for index, normalized_time in enumerate(TIMES):
            # glTF/FBX animation timestamps are absolute from zero, while
            # Blender's imported action range can begin at frame one when the
            # first authored key is at t=0. Sampling between action.frame_range
            # endpoints would therefore shift every interior source sample by
            # one frame. The last action frame is the exact exported duration
            # at this scene's fixed 30 fps, so use the zero-based timeline that
            # the Engine mixer consumes.
            frame = end * normalized_time
            whole = math.floor(frame)
            bpy.context.scene.frame_set(whole, subframe=frame - whole)
            bpy.context.view_layer.update()
            file_name = f"{baseline}-{actor}-{clip}-{index:02d}-{normalized_time:.2f}.png"
            bpy.context.scene.render.filepath = str(output_dir / file_name)
            bpy.ops.render.render(write_still=True)
            bounds_min, bounds_max, vertex_count = evaluated_mesh_bounds()
            image_bytes = (output_dir / file_name).read_bytes()
            records.append(
                {
                    "baseline": baseline,
                    "actor": actor,
                    "clip": clip,
                    "normalizedTime": normalized_time,
                    "sourceFrame": frame,
                    "file": file_name,
                    "imageSha256": hashlib.sha256(image_bytes).hexdigest(),
                    "evaluatedBounds": {
                        "min": [round(value, 6) for value in bounds_min],
                        "max": [round(value, 6) for value in bounds_max],
                    },
                    "vertexCount": vertex_count,
                }
            )
    armature.animation_data.action = None
    return records


def main() -> None:
    args = parse_args()
    root = Path.cwd()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    records: list[dict[str, object]] = []

    for actor in ACTORS:
        reset_scene()
        armature, actions = import_committed_glb(
            root / "content" / "assets" / "actor-kit" / f"{actor}.glb"
        )
        add_camera_and_lights()
        records.extend(render_samples(output_dir, "committed-glb", actor, armature, actions))

    reset_scene()
    armature, actions = import_original_source(args.source_root.resolve())
    add_camera_and_lights()
    records.extend(render_samples(output_dir, "original-fbx", "kenney-source", armature, actions))

    (output_dir / "manifest.json").write_text(
        json.dumps(
            {
                "schemaVersion": 2,
                "sampler": "Blender evaluated dependency graph; no Rusty Engine or Three.js",
                "blenderVersion": bpy.app.version_string,
                "blenderBuildHash": bpy.app.build_hash.decode("ascii"),
                "normalizedTimes": TIMES,
                "inputs": {
                    "committedGlbs": [
                        {
                            "path": f"content/assets/actor-kit/{actor}.glb",
                            "sha256": sha256(
                                root / "content" / "assets" / "actor-kit" / f"{actor}.glb"
                            ),
                        }
                        for actor in ACTORS
                    ],
                    "originalFbx": [
                        {
                            "path": f"Model/characterMedium.fbx",
                            "sha256": sha256(
                                args.source_root.resolve() / "Model" / "characterMedium.fbx"
                            ),
                        },
                        *[
                            {
                                "path": f"Animations/{filename}",
                                "sha256": sha256(
                                    args.source_root.resolve() / "Animations" / filename
                                ),
                            }
                            for filename, _ in SOURCE_CLIPS.values()
                        ],
                    ],
                },
                "samples": records,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"rendered {len(records)} independent Blender samples to {output_dir}")


if __name__ == "__main__":
    main()
