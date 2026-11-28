"""Render preview sheets for a model.

    blender --background models/little-prince/little-prince.blend --python scripts/preview-hero.py -- docs/little-prince

Uses the camera, lights and CameraTarget in the file's Stage collection and
writes turnaround.png, poses.png and walk.png. Poses are the actions marked
as assets; the walk sheet samples the action called Walk, and actions.png
shows one frame of each other looping or one-shot action.
"""

import math
import sys
import tempfile
from pathlib import Path

import bpy
import numpy as np

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
output = Path(args[0] if args else "previews").resolve()
output.mkdir(parents=True, exist_ok=True)
frames = Path(tempfile.mkdtemp(prefix="preview-"))

scene = bpy.context.scene
for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
    try:
        scene.render.engine = engine
        break
    except TypeError:
        continue
scene.eevee.taa_render_samples = 32
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = "PNG"
camera = scene.camera
target = bpy.data.objects["CameraTarget"]
rig = next(obj for obj in scene.objects if obj.type == "ARMATURE")
animation = rig.animation_data


def use_action(name, frame):
    action = bpy.data.actions[name]
    animation.action = action
    if hasattr(animation, "action_slot") and action.slots:
        animation.action_slot = action.slots[0]
    scene.frame_set(frame)


def render(name, angle, size, distance=2.9, height=0.8):
    radians = math.radians(angle)
    camera.location = (distance * math.sin(radians), -distance * math.cos(radians), height)
    scene.render.resolution_x, scene.render.resolution_y = size
    path = frames / f"{name}.png"
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)
    return path


def sheet(name, paths, columns):
    images = [bpy.data.images.load(str(path)) for path in paths]
    width, height = images[0].size
    rows = math.ceil(len(images) / columns)
    canvas = np.ones((rows * height, columns * width, 4), dtype=np.float32)
    for index, image in enumerate(images):
        pixels = np.empty(width * height * 4, dtype=np.float32)
        image.pixels.foreach_get(pixels)
        row, column = divmod(index, columns)
        top = (rows - 1 - row) * height
        canvas[top:top + height, column * width:(column + 1) * width] = pixels.reshape(height, width, 4)
    result = bpy.data.images.new(name, columns * width, rows * height, alpha=False)
    result.pixels.foreach_set(canvas.ravel())
    result.filepath_raw = str(output / f"{name}.png")
    result.file_format = "PNG"
    result.save()
    print(f"wrote {result.filepath_raw}")


poses = [action.name for action in bpy.data.actions if action.asset_data]
poses.sort(key=lambda name: name != "Stand")
use_action(poses[0], 1)
sheet("turnaround", [render(f"turn-{angle}", angle, (480, 640)) for angle in (0, 35, 90, 180)], 4)

pose_paths = []
for pose in poses:
    use_action(pose, 1)
    pose_paths.append(render(f"pose-{pose}", 30, (480, 640)))
sheet("poses", pose_paths, len(pose_paths))

if "Walk" in bpy.data.actions:
    action = bpy.data.actions["Walk"]
    start, end = (int(value) for value in action.frame_range)
    step = (end - start) / 8
    walk_paths = []
    for index in range(8):
        use_action("Walk", round(start + index * step))
        walk_paths.append(render(f"walk-{index}", 90, (360, 480)))
    sheet("walk", walk_paths, 8)

moments = {"Idle": 0.0, "Run": 0.25, "Jump": 0.25, "Land": 0.0, "Cheer": 0.1, "Wave": 0.25}
action_paths = []
for name, moment in moments.items():
    if name in bpy.data.actions:
        start, end = (int(value) for value in bpy.data.actions[name].frame_range)
        use_action(name, round(start + (end - start) * moment))
        action_paths.append(render(f"action-{name}", 50, (360, 480)))
if action_paths:
    sheet("actions", action_paths, len(action_paths))
