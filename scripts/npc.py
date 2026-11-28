"""Turns Quaternius's CC0 animated models into the game's characters.

    blender --background --factory-startup --python scripts/npc.py -- \
        SOURCE.glb OUT.glb fox|lamplighter

Sources (CC0 1.0, by Quaternius, from https://poly.pizza):

- fox: "Fox", https://poly.pizza/m/Bc97C66HKi
- lamplighter: "Worker", https://poly.pizza/m/Yg2bQZO6Hj

The models are coloured by material. This bakes every material's colour
into one palette texture, maps each face to its colour, recolours the worker
into the lamplighter (a navy cap, a blue coat) and keeps only the animations
the game plays, under short names. The result loads like the hero.
"""
import sys

import bpy

ARGS = sys.argv[sys.argv.index("--") + 1:]
SOURCE, OUT, WHO = ARGS[0], ARGS[1], ARGS[2]

# Animations kept, from the source's name to the game's.
KEEP = {
    "fox": {"Idle": "Idle", "Walk": "Walk", "Gallop": "Run", "Idle_2_HeadLow": "Sniff", "Idle_2": "Look"},
    "lamplighter": {"Idle": "Idle", "Walk": "Walk", "Run": "Run", "Interact": "Light", "Wave": "Wave", "Idle_Neutral": "Rest"},
}[WHO]

# New colours by material name, as sRGB hex.
RECOLOUR = {
    "fox": {},
    "lamplighter": {
        "Worker_Yellow": 0x2B3566,
        "Worker_Vest": 0x3F5CA8,
        "Grey": 0xE8E2D6,
        "LightBrown": 0x3A3A4E,
    },
}[WHO]


def srgb(c):
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055


for o in list(bpy.data.objects):
    bpy.data.objects.remove(o)
bpy.ops.import_scene.gltf(filepath=SOURCE)

meshes = [o for o in bpy.data.objects if o.type == "MESH"]
colours = []
index = {}
for o in meshes:
    for slot in o.material_slots:
        m = slot.material
        if m is None or m.name in index:
            continue
        hexc = RECOLOUR.get(m.name.split(".")[0])
        if hexc is not None:
            rgb = [((hexc >> s) & 255) / 255 for s in (16, 8, 0)]
        else:
            bsdf = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
            base = bsdf.inputs["Base Color"].default_value if bsdf else m.diffuse_color
            rgb = [srgb(base[i]) for i in range(3)]
        index[m.name] = len(colours)
        colours.append(rgb)

W = 16
assert len(colours) <= W * W, "too many materials"
H = max(1, -(-len(colours) // W))
img = bpy.data.images.new("palette", W, H, alpha=False)
px = [0.0] * (W * H * 4)
for i, c in enumerate(colours):
    x, y = i % W, i // W
    px[(y * W + x) * 4:(y * W + x) * 4 + 4] = [c[0], c[1], c[2], 1.0]
img.pixels = px
img.filepath_raw = f"/tmp/{WHO}-palette.png"
img.file_format = "PNG"
img.save()

mat = bpy.data.materials.new("Palette")
mat.use_nodes = True
nodes = mat.node_tree.nodes
bsdf = nodes["Principled BSDF"]
tex = nodes.new("ShaderNodeTexImage")
tex.image = img
tex.interpolation = "Closest"
mat.node_tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
bsdf.inputs["Roughness"].default_value = 1.0

for o in meshes:
    me = o.data
    names = [s.material.name if s.material else None for s in o.material_slots]
    while me.uv_layers:
        me.uv_layers.remove(me.uv_layers[0])
    uv = me.uv_layers.new(name="palette")
    for poly in me.polygons:
        i = index.get(names[poly.material_index] if poly.material_index < len(names) else None, 0)
        u, v = (i % W + 0.5) / W, 1.0 - (i // W + 0.5) / H
        for li in poly.loop_indices:
            uv.data[li].uv = (u, v)
    me.materials.clear()
    me.materials.append(mat)

# The armature comes rotated a quarter turn and scaled by 100, which the
# game does not apply to skinned meshes: bake it into the bones and meshes,
# and scale the animations' moves to match.
arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
factor = arm.scale[0]
bpy.ops.object.select_all(action="DESELECT")
for o in [arm] + [c for c in arm.children_recursive]:
    o.select_set(True)
bpy.context.view_layer.objects.active = arm
bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
for a in bpy.data.actions:
    for fc in a.fcurves:
        if fc.data_path.endswith(".location"):
            for kp in fc.keyframe_points:
                kp.co[1] *= factor
                kp.handle_left[1] *= factor
                kp.handle_right[1] *= factor

# One copy of each kept animation, under its game name.
chosen = {}
for a in list(bpy.data.actions):
    short = a.name.split("|")[-1]
    if short in KEEP and short not in chosen:
        chosen[short] = a
    else:
        bpy.data.actions.remove(a)
for a in chosen.values():
    a.name = "tmp-" + a.name
for short, a in chosen.items():
    a.name = KEEP[short]
kept = {a.name for a in bpy.data.actions}
for o in bpy.data.objects:
    if o.animation_data:
        o.animation_data.action = None
        for t in list(o.animation_data.nla_tracks):
            o.animation_data.nla_tracks.remove(t)
missing = set(KEEP.values()) - kept
assert not missing, f"missing animations {missing}"

bpy.ops.export_scene.gltf(
    filepath=OUT,
    export_format="GLB",
    export_animation_mode="ACTIONS",
    export_texcoords=True,
    export_normals=True,
    export_materials="EXPORT",
    export_yup=True,
)
tris = sum(len(p.vertices) - 2 for o in meshes for p in o.data.polygons)
print(f"{OUT}: {len(colours)} colours, {tris} triangles, animations {sorted(kept)}")
