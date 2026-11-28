"""Export the Little Prince's Blender model.

    blender --background --factory-startup --python-exit-code 1 --python scripts/export-hero.py
    blender --background --factory-startup --python-exit-code 1 --python scripts/export-hero.py -- --smooth

The default writes assets/hero/little-prince.glb with no subdivision, meshes
decimated according to their game_detail properties, and a single palette
texture. --smooth writes dist/little-prince.glb with the original materials
and subdivision levels. --subdivision N writes dist/little-prince-subN.glb
with the given subdivision level and the original materials.
"""

import argparse
import json
import struct
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

import bpy

ROOT = Path(__file__).resolve().parent.parent
BLEND = ROOT / "models/little-prince/little-prince.blend"
SWATCH = 8
COLUMNS = 8
GAME_LIMITS = {"triangles": 6000, "vertices": 65535, "joints": 64}


def parse_args():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description="Export the Little Prince's Blender model")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--smooth", action="store_true")
    mode.add_argument("--subdivision", type=int)
    options = parser.parse_args(args)
    if options.subdivision is not None and options.subdivision < 0:
        parser.error("--subdivision must be non-negative")
    return options


def read_glb(path):
    with open(path, "rb") as file:
        file.read(12)
        length, _ = struct.unpack("<I4s", file.read(8))
        return json.loads(file.read(length))


def exported_actions(objects):
    """The actions the objects play: active, or on their NLA tracks."""
    actions = set()
    for obj in objects:
        animation = obj.animation_data
        if animation is None:
            continue
        if animation.action:
            actions.add(animation.action)
        for track in animation.nla_tracks:
            actions.update(strip.action for strip in track.strips if strip.action)
    return actions


def action_lengths(objects):
    """Seconds each exported action lasts; one-frame poses are left out."""
    fps = bpy.context.scene.render.fps / bpy.context.scene.render.fps_base
    lengths = {}
    for action in exported_actions(objects):
        frames = action.frame_range[1] - action.frame_range[0]
        if frames > 1:
            lengths[action.name] = frames / fps
    return lengths


def check_lengths(gltf, expected):
    problems = []
    for animation in gltf.get("animations", []):
        length = max(gltf["accessors"][sampler["input"]]["max"][0] for sampler in animation["samplers"])
        want = expected.get(animation["name"])
        if want is not None and abs(length - want) > 0.5 / 30:
            problems.append(f"{animation['name']} lasts {length:.3f} s, its action {want:.3f} s")
    missing = set(expected) - {animation["name"] for animation in gltf.get("animations", [])}
    problems += [f"{name} was not exported" for name in sorted(missing)]
    return problems


def summarize(path, expected):
    gltf = read_glb(path)
    primitives = [primitive for mesh in gltf.get("meshes", []) for primitive in mesh["primitives"]]
    stats = {
        "triangles": sum(gltf["accessors"][primitive["indices"]]["count"] // 3 for primitive in primitives),
        "vertices": sum(gltf["accessors"][primitive["attributes"]["POSITION"]]["count"] for primitive in primitives),
        "joints": sum(len(skin["joints"]) for skin in gltf.get("skins", [])),
    }
    animations = [animation["name"] for animation in gltf.get("animations", [])]
    size = path.stat().st_size / 1024 / 1024
    print(f"{path.relative_to(ROOT)}: {size:.2f} MB, {stats['triangles']} triangles, "
          f"{stats['vertices']} vertices, {stats['joints']} joints")
    print(f"  animations: {', '.join(animations) or 'none'}")
    problems = check_lengths(gltf, expected)
    if problems:
        sys.exit(f"{path.name}: " + "; ".join(problems))
    return gltf, primitives, stats


def check_game(path, expected):
    gltf, primitives, stats = summarize(path, expected)
    problems = [f"{key} {stats[key]} > {limit}" for key, limit in GAME_LIMITS.items() if stats[key] > limit]
    for primitive in primitives:
        attributes = primitive["attributes"]
        for name in ("NORMAL", "TEXCOORD_0", "JOINTS_0", "WEIGHTS_0"):
            if name not in attributes:
                problems.append(f"a primitive has no {name}")
        material = gltf["materials"][primitive["material"]] if "material" in primitive else {}
        if "baseColorTexture" not in material.get("pbrMetallicRoughness", {}):
            problems.append("a primitive has no base colour texture")
    if len(gltf.get("skins", [])) != 1:
        problems.append("expected exactly one skin")
    if not gltf.get("animations"):
        problems.append("no animations")
    if problems:
        sys.exit(f"{path.name} is not game-ready: " + "; ".join(sorted(set(problems))))


def to_srgb(value):
    return value * 12.92 if value <= 0.0031308 else 1.055 * value ** (1 / 2.4) - 0.055


def base_color(material):
    if material.use_nodes:
        for node in material.node_tree.nodes:
            if node.type == "BSDF_PRINCIPLED":
                return tuple(node.inputs["Base Color"].default_value[:3])
    return tuple(material.diffuse_color[:3])


def bake_palette(meshes, name):
    materials = []
    for obj in meshes:
        for slot in obj.material_slots:
            if slot.material and slot.material not in materials:
                materials.append(slot.material)
    rows = (len(materials) + COLUMNS - 1) // COLUMNS
    width, height = COLUMNS * SWATCH, max(rows, 1) * SWATCH
    image = bpy.data.images.new(f"{name}-palette", width, height, alpha=False)
    pixels = [1.0] * (width * height * 4)
    for index, material in enumerate(materials):
        row, column = divmod(index, COLUMNS)
        color = [to_srgb(channel) for channel in base_color(material)]
        for y in range(row * SWATCH, (row + 1) * SWATCH):
            for x in range(column * SWATCH, (column + 1) * SWATCH):
                start = (y * width + x) * 4
                pixels[start:start + 3] = color
    image.pixels.foreach_set(pixels)
    image.pack()

    palette = bpy.data.materials.new(f"{name}-palette")
    palette.use_nodes = True
    nodes = palette.node_tree.nodes
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    texture.interpolation = "Closest"
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.6
    palette.node_tree.links.new(texture.outputs["Color"], bsdf.inputs["Base Color"])

    for obj in meshes:
        mesh = obj.data
        uv = mesh.uv_layers.new(name="Palette")
        for polygon in mesh.polygons:
            material = obj.material_slots[polygon.material_index].material
            row, column = divmod(materials.index(material), COLUMNS)
            u = (column + 0.5) * SWATCH / width
            v = (row + 0.5) * SWATCH / height
            for loop in polygon.loop_indices:
                uv.data[loop].uv = (u, v)
        for layer in list(mesh.uv_layers):
            if layer.name != "Palette":
                mesh.uv_layers.remove(layer)
        mesh.materials.clear()
        mesh.materials.append(palette)
        mesh.polygons.foreach_set("material_index", [0] * len(mesh.polygons))


def apply_modifier(obj, modifier):
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier=modifier.name)


def prepare_game(layer, name):
    meshes = [obj for obj in layer.collection.all_objects if obj.type == "MESH"]
    for obj in meshes:
        for modifier in list(obj.modifiers):
            if modifier.type == "SUBSURF":
                obj.modifiers.remove(modifier)
        detail = float(obj.get("game_detail", 1.0))
        if detail < 1.0:
            decimate = obj.modifiers.new("Decimate", "DECIMATE")
            decimate.ratio = detail
            decimate.use_collapse_triangulate = True
            obj.modifiers.move(len(obj.modifiers) - 1, 0)
            apply_modifier(obj, decimate)
    bake_palette(meshes, name)


def main():
    options = parse_args()
    bpy.ops.wm.open_mainfile(filepath=str(BLEND))
    layer = bpy.context.view_layer.layer_collection.children.get("Export")
    if layer is None:
        sys.exit(f"{BLEND}: no collection called Export")
    bpy.context.view_layer.active_layer_collection = layer
    expected = action_lengths(layer.collection.all_objects)
    game = not options.smooth and options.subdivision is None
    if game:
        output = ROOT / "assets/hero/little-prince.glb"
        prepare_game(layer, BLEND.stem)
    elif options.subdivision is not None:
        output = ROOT / "dist" / f"little-prince-sub{options.subdivision}.glb"
        for obj in layer.collection.all_objects:
            for modifier in obj.modifiers:
                if modifier.type == "SUBSURF":
                    modifier.levels = options.subdivision
    else:
        output = ROOT / "dist/little-prince.glb"
    output.parent.mkdir(exist_ok=True)
    with TemporaryDirectory(dir=output.parent) as directory:
        staged = Path(directory) / output.name
        bpy.ops.export_scene.gltf(
            filepath=str(staged),
            export_format="GLB",
            use_active_collection=True,
            use_active_collection_with_nested=True,
            export_apply=True,
            export_animation_mode="ACTIONS",
            export_frame_range=False,
            export_anim_slide_to_zero=True,
            export_image_format="AUTO",
        )
        if game:
            check_game(staged, expected)
        else:
            summarize(staged, expected)
        staged.replace(output)
    print(f"Wrote {output.relative_to(ROOT)}")


main()
