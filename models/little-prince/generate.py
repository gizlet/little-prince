"""Generate the first version of little-prince.blend from scratch.

The .blend file is the source of truth. Running this script again overwrites
it, so use it only to start over.

    blender --background --factory-startup --python models/little-prince/generate.py
"""

import math
import random
from pathlib import Path

import bpy
from mathutils import Euler, Matrix, Quaternion, Vector

OUTPUT = Path(__file__).resolve().parent / "little-prince.blend"
FPS = 30
UP = Vector((0, 0, 1))
SIDE = Vector((1, 0, 0))
DEPTH = Vector((0, 1, 0))
rng = random.Random(612)


def srgb(hex_color):
    channels = [int(hex_color[index:index + 2], 16) / 255 for index in (1, 3, 5)]
    return tuple(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in channels)


def smoothstep(edge0, edge1, value):
    t = min(max((value - edge0) / (edge1 - edge0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def lerp(a, b, t):
    return a + (b - a) * t


# Skeleton: name, head, tail, parent, connected, deform. Left side only; the
# right side is mirrored. The character stands on z = 0 and faces -Y.
LEFT_BONES = [
    ("root", (0, 0, 0), (0, 0.16, 0), None, False, False),
    ("hips", (0, 0, 0.47), (0, 0, 0.54), "root", False, True),
    ("spine", (0, 0, 0.54), (0, 0, 0.61), "hips", True, True),
    ("chest", (0, 0, 0.61), (0, 0, 0.69), "spine", True, True),
    ("neck", (0, 0, 0.69), (0, 0.004, 0.745), "chest", True, True),
    ("head", (0, 0.004, 0.745), (0, 0.004, 0.97), "neck", True, True),
    ("shoulder.L", (0.02, 0, 0.672), (0.085, 0, 0.668), "chest", False, True),
    ("upper_arm.L", (0.085, 0, 0.668), (0.15, 0.006, 0.55), "shoulder.L", True, True),
    ("forearm.L", (0.15, 0.006, 0.55), (0.193, -0.01, 0.448), "upper_arm.L", True, True),
    ("hand.L", (0.193, -0.01, 0.448), (0.219, -0.0235, 0.376), "forearm.L", True, True),
    ("thigh.L", (0.058, 0, 0.465), (0.06, -0.006, 0.257), "hips", False, True),
    ("shin.L", (0.06, -0.006, 0.257), (0.062, 0.006, 0.066), "thigh.L", True, True),
    ("foot.L", (0.062, 0.006, 0.066), (0.064, -0.05, 0.02), "shin.L", True, True),
    ("toe.L", (0.064, -0.05, 0.02), (0.065, -0.098, 0.02), "foot.L", True, True),
    ("scarf_front", (0.026, -0.064, 0.672), (0.04, -0.086, 0.555), "chest", False, True),
    ("scarf_back.1", (0.07, 0.064, 0.69), (0.052, 0.079, 0.625), "chest", False, True),
    ("scarf_back.2", (0.052, 0.079, 0.625), (0.04, 0.083, 0.56), "scarf_back.1", True, True),
    ("scarf_back.3", (0.04, 0.083, 0.56), (0.035, 0.088, 0.5), "scarf_back.2", True, True),
    ("scarf_back.4", (0.035, 0.088, 0.5), (0.035, 0.092, 0.435), "scarf_back.3", True, True),
]


def mirror_name(name):
    if name.endswith(".L"):
        return name[:-2] + ".R"
    if name.endswith(".R"):
        return name[:-2] + ".L"
    return name


def all_bones():
    bones = list(LEFT_BONES)
    for name, head, tail, parent, connect, deform in LEFT_BONES:
        if name.endswith(".L"):
            bones.append((
                mirror_name(name),
                (-head[0], head[1], head[2]),
                (-tail[0], tail[1], tail[2]),
                mirror_name(parent),
                connect,
                deform,
            ))
    return bones


SEGMENTS = {name: (Vector(head), Vector(tail)) for name, head, tail, *_ in all_bones()}


def segment_distance(point, start, end):
    span = end - start
    t = max(0.0, min(1.0, (point - start).dot(span) / span.length_squared))
    return (point - (start + span * t)).length


def normalized(raw, keep=4, floor=0.02):
    items = sorted(raw.items(), key=lambda item: -item[1])[:keep]
    total = sum(value for _, value in items)
    items = [(name, value / total) for name, value in items if value / total >= floor]
    total = sum(value for _, value in items)
    return {name: value / total for name, value in items}


def envelope(*bones, power=4.0):
    def weights(point):
        return normalized({
            bone: 1.0 / (segment_distance(point, *SEGMENTS[bone]) + 0.004) ** power
            for bone in bones
        })
    return weights


def rigid(bone):
    return lambda point: {bone: 1.0}


def torso_weights(point):
    centers = [("hips", 0.47), ("spine", 0.575), ("chest", 0.65), ("neck", 0.715)]
    raw = {}
    z = min(max(point.z, centers[0][1]), centers[-1][1])
    for (low_name, low), (high_name, high) in zip(centers, centers[1:]):
        if low <= z <= high:
            t = smoothstep(low, high, z)
            raw[low_name] = raw.get(low_name, 0) + 1 - t
            raw[high_name] = raw.get(high_name, 0) + t
            break
    shoulder = smoothstep(0.055, 0.1, abs(point.x)) * smoothstep(0.62, 0.675, point.z) * 0.7
    if shoulder > 0:
        raw = {name: value * (1 - shoulder) for name, value in raw.items()}
        raw["shoulder.L" if point.x > 0 else "shoulder.R"] = shoulder
    thigh = smoothstep(0.475, 0.43, point.z) * 0.55
    if thigh > 0:
        raw = {name: value * (1 - thigh) for name, value in raw.items()}
        left = min(max(0.5 + point.x / 0.06, 0.0), 1.0)
        raw["thigh.L"] = thigh * left
        raw["thigh.R"] = thigh * (1 - left)
    return normalized({name: value for name, value in raw.items() if value > 0})


def boot_weights(point):
    shin = smoothstep(0.07, 0.095, point.z)
    toe = smoothstep(-0.042, -0.066, point.y)
    return normalized({
        "shin.L": shin + 1e-6,
        "foot.L": (1 - shin) * (1 - toe) + 1e-6,
        "toe.L": (1 - shin) * toe + 1e-6,
    })


class Part:
    def __init__(self, name, subsurf=1):
        self.name = name
        self.subsurf = subsurf
        self.verts = []
        self.faces = []
        self.face_materials = []
        self.weights = []
        self.materials = []

    def add(self, verts, faces, material, weights, mirror=False):
        if material not in self.materials:
            self.materials.append(material)
        index = self.materials.index(material)
        computed = [weights(vert) for vert in verts]
        copies = [(verts, faces, computed)]
        if mirror:
            copies.append((
                [Vector((-vert.x, vert.y, vert.z)) for vert in verts],
                [list(reversed(face)) for face in faces],
                [{mirror_name(name): value for name, value in entry.items()} for entry in computed],
            ))
        for copy_verts, copy_faces, copy_weights in copies:
            base = len(self.verts)
            self.verts.extend(copy_verts)
            self.weights.extend(copy_weights)
            for face in copy_faces:
                self.faces.append([base + vertex for vertex in face])
                self.face_materials.append(index)

    def build(self, collection, armature):
        mesh = bpy.data.meshes.new(self.name)
        mesh.from_pydata([tuple(vert) for vert in self.verts], [], self.faces)
        for material in self.materials:
            mesh.materials.append(material)
        mesh.polygons.foreach_set("material_index", self.face_materials)
        mesh.polygons.foreach_set("use_smooth", [True] * len(self.faces))
        mesh.validate()
        mesh.update()
        obj = bpy.data.objects.new(self.name, mesh)
        collection.objects.link(obj)
        obj.parent = armature
        groups = {}
        for vertex, entry in enumerate(self.weights):
            for bone, value in entry.items():
                if bone not in groups:
                    groups[bone] = obj.vertex_groups.new(name=bone)
                groups[bone].add([vertex], value, "REPLACE")
        if self.subsurf:
            modifier = obj.modifiers.new("Subdivision", "SUBSURF")
            modifier.levels = self.subsurf
            modifier.render_levels = self.subsurf + 1
        modifier = obj.modifiers.new("Armature", "ARMATURE")
        modifier.object = armature
        return obj


def tube(part, material, weights, points, radii, segments=12, side=SIDE,
         round_start=False, round_end=False, cap_start=None, cap_end=None,
         closed=False, round_length=None, mirror=False):
    """Loft ellipses along points. radii are numbers or (along side, across) pairs."""
    points = [Vector(point) for point in points]
    radii = [(r, r) if isinstance(r, (int, float)) else tuple(r) for r in radii]
    count = len(points)
    tangents = []
    previous = None
    for index in range(count):
        if closed:
            direction = points[(index + 1) % count] - points[(index - 1) % count]
        else:
            direction = points[min(index + 1, count - 1)] - points[max(index - 1, 0)]
        if direction.length < 1e-7:
            direction = previous.copy() if previous is not None else UP.copy()
        direction.normalize()
        if previous is not None and direction.dot(previous) < 0:
            direction = -direction
        tangents.append(direction)
        previous = direction
    rings = [(points[i], tangents[i], radii[i]) for i in range(count)]

    def rounded(center, tangent, size, sign, angles):
        length = round_length if round_length is not None else (size[0] + size[1]) / 2
        result = []
        for angle in angles:
            a = math.radians(angle)
            result.append((center + tangent * (sign * length * math.sin(a)), tangent,
                           (size[0] * math.cos(a), size[1] * math.cos(a))))
        return result, center + tangent * (sign * length)

    tip_start = tip_end = None
    if round_start:
        extra, tip_start = rounded(*rings[0], -1, (72, 48, 22))
        rings = extra + rings
    elif cap_start == "flat":
        tip_start = rings[0][0]
    elif cap_start is not None:
        tip_start = Vector(cap_start)
    if round_end:
        extra, tip_end = rounded(*rings[-1], 1, (22, 48, 72))
        rings = rings + extra
    elif cap_end == "flat":
        tip_end = rings[-1][0]
    elif cap_end is not None:
        tip_end = Vector(cap_end)

    verts = []
    faces = []
    for center, tangent, (rx, ry) in rings:
        hint = side(center) if callable(side) else side
        axis = hint - tangent * hint.dot(tangent)
        if axis.length < 1e-6:
            axis = tangent.orthogonal()
        axis.normalize()
        across = tangent.cross(axis)
        for k in range(segments):
            a = 2 * math.pi * k / segments
            verts.append(center + axis * (math.cos(a) * rx) + across * (math.sin(a) * ry))
    ring_count = len(rings)
    for i in range(ring_count - (0 if closed else 1)):
        j = (i + 1) % ring_count
        for k in range(segments):
            k2 = (k + 1) % segments
            faces.append([i * segments + k, i * segments + k2, j * segments + k2, j * segments + k])
    if tip_start is not None:
        tip = len(verts)
        verts.append(tip_start)
        faces.extend([[tip, (k + 1) % segments, k] for k in range(segments)])
    if tip_end is not None:
        tip = len(verts)
        verts.append(tip_end)
        base = (ring_count - 1) * segments
        faces.extend([[tip, base + k, base + (k + 1) % segments] for k in range(segments)])
    part.add(verts, faces, material, weights, mirror)


def ellipsoid(part, material, weights, center, radii, rotation=None, segments=16,
              rings=10, mirror=False, deform=None):
    if rotation is None:
        matrix = Matrix.Identity(3)
    elif isinstance(rotation, Quaternion):
        matrix = rotation.to_matrix()
    else:
        matrix = rotation
    rx, ry, rz = radii
    verts = []
    for i in range(1, rings):
        theta = math.pi * i / rings
        for k in range(segments):
            a = 2 * math.pi * k / segments
            verts.append(Vector((rx * math.sin(theta) * math.cos(a),
                                 ry * math.sin(theta) * math.sin(a),
                                 -rz * math.cos(theta))))
    bottom = len(verts)
    verts.append(Vector((0, 0, -rz)))
    top = len(verts)
    verts.append(Vector((0, 0, rz)))
    faces = []
    for i in range(rings - 2):
        for k in range(segments):
            k2 = (k + 1) % segments
            faces.append([i * segments + k, i * segments + k2, (i + 1) * segments + k2, (i + 1) * segments + k])
    last = (rings - 2) * segments
    for k in range(segments):
        k2 = (k + 1) % segments
        faces.append([bottom, k2, k])
        faces.append([top, last + k, last + k2])
    verts = [Vector(center) + matrix @ vert for vert in verts]
    if deform:
        verts = [deform(vert) for vert in verts]
    part.add(verts, faces, material, weights, mirror)


def facing(normal):
    return Vector((0, -1, 0)).rotation_difference(normal)


def material(name, hex_color, roughness=0.6, **inputs):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    color = srgb(hex_color)
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1)
    bsdf.inputs["Roughness"].default_value = roughness
    for key, value in inputs.items():
        bsdf.inputs[key.replace("_", " ")].default_value = value
    mat.diffuse_color = (*color, 1)
    mat.roughness = roughness
    return mat


# Head shape: an egg with a softer jaw, round cheeks and a fuller back.
HEAD_CENTER = Vector((0, 0.004, 0.842))
HEAD_RADII = Vector((0.104, 0.1, 0.112))


def head_deform(point):
    local = point - HEAD_CENTER
    nx = local.x / HEAD_RADII.x
    ny = local.y / HEAD_RADII.y
    nz = local.z / HEAD_RADII.z
    if nz < 0:
        jaw = (-nz) ** 1.6
        local.x *= 1 - 0.07 * jaw
        local.y *= 1 - 0.03 * jaw
        local.z *= 1 - 0.08 * jaw
    cheek = math.exp(-((abs(nx) - 0.55) ** 2 + (nz + 0.3) ** 2) / 0.05) * max(0.0, -ny)
    local += Vector((math.copysign(0.005, nx), -0.006, -0.002)) * cheek
    if ny > 0:
        local.y *= 1 + 0.06 * smoothstep(-0.4, 0.4, nz)
    return HEAD_CENTER + local


def head_point(nx, nz, out=0.0):
    ny = -math.sqrt(max(0.0, 1 - nx * nx - nz * nz))
    point = HEAD_CENTER + Vector((nx * HEAD_RADII.x, ny * HEAD_RADII.y, nz * HEAD_RADII.z))
    normal = Vector((nx / HEAD_RADII.x, ny / HEAD_RADII.y, nz / HEAD_RADII.z)).normalized()
    return head_deform(point) + normal * out, normal


TORSO = [
    # z, half width, half depth, y offset
    (0.43, 0.074, 0.06, 0.0),
    (0.46, 0.094, 0.07, 0.0),
    (0.50, 0.097, 0.072, 0.0),
    (0.55, 0.099, 0.075, -0.003),
    (0.60, 0.101, 0.073, -0.001),
    (0.64, 0.103, 0.068, 0.003),
    (0.665, 0.097, 0.062, 0.005),
    (0.685, 0.074, 0.052, 0.006),
    (0.70, 0.047, 0.044, 0.004),
    (0.714, 0.043, 0.041, 0.004),
]


def torso_at(z):
    for low, high in zip(TORSO, TORSO[1:]):
        if low[0] <= z <= high[0]:
            t = (z - low[0]) / (high[0] - low[0])
            return tuple(lerp(a, b, t) for a, b in zip(low[1:], high[1:]))
    return TORSO[-1][1:] if z > TORSO[-1][0] else TORSO[0][1:]


def build_suit(part, mats):
    points = [(0, yoff, z) for z, _, _, yoff in TORSO]
    radii = [(rx, ry) for _, rx, ry, _ in TORSO]
    points += [(0, 0.004, 0.714), (0, 0.004, 0.69)]
    radii += [(0.035, 0.033), (0.033, 0.031)]
    tube(part, mats["suit"], torso_weights, points, radii, segments=20, cap_start=(0, 0, 0.418))

    belt_z = 0.5
    rx, ry, yoff = torso_at(belt_z)
    loop = [Vector(((rx + 0.003) * math.cos(a), (ry + 0.003) * math.sin(a) + yoff, belt_z))
            for a in (2 * math.pi * i / 28 for i in range(28))]
    centre = Vector((0, yoff, belt_z))
    tube(part, mats["belt"], torso_weights, loop, [(0.0045, 0.011)] * len(loop), segments=8,
         side=lambda p: (p - centre) * Vector((1, 1, 0)), closed=True)
    buckle_y = -(ry + 0.008) + yoff
    ellipsoid(part, mats["buckle"], torso_weights, (0, buckle_y, belt_z), (0.017, 0.005, 0.014),
              segments=12, rings=6)
    for z in (0.565, 0.605, 0.642):
        _, ry_z, yoff_z = torso_at(z)
        ellipsoid(part, mats["button"], torso_weights, (0, -ry_z + yoff_z - 0.0015, z),
                  (0.0065, 0.003, 0.0065), segments=10, rings=6)

    sleeve = [(0.05, 0.0, 0.667), (0.085, 0.0, 0.668), (0.118, 0.003, 0.61), (0.15, 0.006, 0.55),
              (0.172, -0.002, 0.498), (0.19, -0.009, 0.455), (0.193, -0.01, 0.448),
              (0.193, -0.01, 0.448), (0.188, -0.008, 0.46)]
    sleeve_radii = [0.034, 0.039, 0.035, 0.032, 0.03, 0.031, 0.032, 0.023, 0.022]
    tube(part, mats["suit"],
         envelope("chest", "shoulder.L", "upper_arm.L", "forearm.L", "hand.L"),
         sleeve, sleeve_radii, segments=12, side=DEPTH, cap_start="flat", mirror=True)

    leg = [(0.048, 0, 0.53), (0.056, 0, 0.465), (0.058, -0.002, 0.36), (0.06, -0.004, 0.258),
           (0.061, 0.0, 0.16), (0.062, 0.004, 0.09), (0.062, 0.006, 0.042), (0.062, 0.006, 0.034),
           (0.062, 0.006, 0.034), (0.062, 0.006, 0.09)]
    leg_radii = [(0.045, 0.043), (0.055, 0.056), (0.055, 0.056), (0.05, 0.052), (0.056, 0.057),
                 (0.068, 0.066), (0.078, 0.074), (0.08, 0.076), (0.07, 0.066), (0.062, 0.058)]
    tube(part, mats["suit"], envelope("hips", "thigh.L", "shin.L"), leg, leg_radii,
         segments=16, cap_start="flat", mirror=True)


def build_boots(part, mats):
    shaft = [(0.062, 0.008, 0.14), (0.062, 0.008, 0.07), (0.062, 0.01, 0.045)]
    tube(part, mats["boot"], boot_weights, shaft, [0.032, 0.033, 0.034], segments=12,
         cap_start="flat", mirror=True)
    foot = [(0.062, 0.042, 0.035), (0.062, 0.012, 0.045), (0.063, -0.025, 0.033),
            (0.065, -0.06, 0.027), (0.066, -0.086, 0.026)]
    foot_radii = [(0.029, 0.034), (0.034, 0.04), (0.036, 0.031), (0.034, 0.025), (0.029, 0.021)]
    verts_before = len(part.verts)
    tube(part, mats["boot"], boot_weights, foot, foot_radii, segments=14,
         round_start=True, round_end=True, mirror=True)
    for vert in part.verts[verts_before:]:
        vert.z = max(vert.z, 0.0)


HAND_SCALE = 1.4


def build_hand(part, mats):
    size = HAND_SCALE
    wrist, tip = SEGMENTS["hand.L"]
    along = (tip - wrist).normalized()
    thumb_side = (Vector((0, -1, 0)) - along * along.dot(Vector((0, -1, 0)))).normalized()
    palm_normal = along.cross(thumb_side).normalized()
    basis = Matrix((thumb_side, palm_normal, along)).transposed()
    weights = rigid("hand.L")
    palm = wrist + along * 0.018 * size
    ellipsoid(part, mats["skin"], weights, palm, (0.019 * size, 0.0105 * size, 0.021 * size),
              rotation=basis, segments=12, rings=8, mirror=True)
    for offset, length in ((0.011, 0.021), (0.004, 0.025), (-0.003, 0.024), (-0.01, 0.019)):
        length *= size
        base = palm + along * 0.013 * size + thumb_side * offset * size
        points = [base, base + along * length * 0.5 + palm_normal * length * 0.08,
                  base + along * length * 0.85 + palm_normal * length * 0.32]
        tube(part, mats["skin"], weights, points,
             [0.0056 * size, 0.0052 * size, 0.0048 * size], segments=8,
             round_start=True, round_end=True, mirror=True)
    base = palm + (thumb_side * 0.013 + along * -0.004 + palm_normal * 0.003) * size
    direction = (thumb_side * 0.55 + along * 0.8 + palm_normal * 0.3).normalized()
    points = [base, base + direction * 0.011 * size,
              base + (direction * 0.02 + palm_normal * 0.003) * size]
    tube(part, mats["skin"], weights, points, [0.0065 * size, 0.006 * size, 0.0055 * size],
         segments=8, round_start=True, round_end=True, mirror=True)


def build_head(part, mats):
    head = rigid("head")
    ellipsoid(part, mats["skin"], head, HEAD_CENTER, HEAD_RADII, segments=24, rings=16,
              deform=head_deform)
    tube(part, mats["skin"], envelope("chest", "neck", "head"),
         [(0, 0.004, 0.67), (0, 0.004, 0.72), (0, 0.006, 0.775)], [0.031, 0.03, 0.032],
         segments=12, cap_start="flat", cap_end="flat")
    ellipsoid(part, mats["skin"], head, (0.099, 0.012, 0.826), (0.013, 0.019, 0.026),
              rotation=Euler((0, math.radians(-12), math.radians(-10))).to_matrix(),
              segments=10, rings=8, mirror=True)
    nose, normal = head_point(0.0, -0.2, out=-0.004)
    ellipsoid(part, mats["skin"], head, nose, (0.008, 0.008, 0.0075), rotation=facing(normal),
              segments=10, rings=8)
    for side in (1, -1):
        eye, normal = head_point(0.3 * side, -0.06, out=-0.0024)
        ellipsoid(part, mats["eye"], head, eye, (0.0068, 0.004, 0.0085),
                  rotation=facing(normal), segments=12, rings=8)
    cheek, normal = head_point(0.5, -0.32, out=-0.0044)
    ellipsoid(part, mats["cheek"], head, cheek, (0.019, 0.0048, 0.014), rotation=facing(normal),
              segments=14, rings=6, mirror=True)
    brow = [head_point(nx, nz, out=0.0012)[0] for nx, nz in ((0.21, 0.1), (0.3, 0.13), (0.4, 0.12))]
    tube(part, mats["brow"], head, brow, [0.0022, 0.0025, 0.0018], segments=8,
         round_start=True, round_end=True, mirror=True)
    mouth = []
    for i in range(5):
        nx = lerp(-0.08, 0.08, i / 4)
        mouth.append(head_point(nx, -0.44 + 2.0 * nx * nx, out=0.0006)[0])
    tube(part, mats["mouth"], head, mouth, [0.0016, 0.002, 0.0022, 0.002, 0.0016], segments=8,
         round_start=True, round_end=True)


HAIR_CENTER = HEAD_CENTER + Vector((0, 0.002, 0.008))
HAIR_RADII = Vector((0.114, 0.11, 0.118))


def hair_edge(phi):
    front = -math.sin(phi)
    boundary = -0.05 + 0.36 * front if front > 0 else -0.05 + 0.68 * front
    return math.acos(boundary)


def hair_point(theta, phi, scale=1.0):
    local = Vector((HAIR_RADII.x * math.sin(theta) * math.cos(phi),
                    HAIR_RADII.y * math.sin(theta) * math.sin(phi),
                    HAIR_RADII.z * math.cos(theta))) * scale
    return head_deform(HAIR_CENTER + local)


def build_hair(part, mats):
    head = rigid("head")
    segments = 28
    rings = 7
    verts = [hair_point(0, 0)]
    for i in range(1, rings + 2):
        for k in range(segments):
            phi = 2 * math.pi * k / segments
            edge = hair_edge(phi)
            if i <= rings:
                verts.append(hair_point(edge * i / rings, phi))
            else:
                verts.append(hair_point(edge + 0.06, phi, 0.95))
    faces = [[0, 1 + k, 1 + (k + 1) % segments] for k in range(segments)]
    for i in range(rings):
        for k in range(segments):
            k2 = (k + 1) % segments
            a = 1 + i * segments
            b = 1 + (i + 1) * segments
            faces.append([a + k, b + k, b + k2, a + k2])
    part.add(verts, faces, mats["hair"], head)

    tufts = []
    for k in range(8):
        phi = 2 * math.pi * k / 8 + rng.uniform(-0.2, 0.2)
        tufts.append(("crown", rng.uniform(0.18, 0.38), phi))
    for k in range(16):
        phi = 2 * math.pi * (k + 0.5) / 16 + rng.uniform(-0.1, 0.1)
        tufts.append(("middle", rng.uniform(0.5, 0.68), phi))
    for k in range(30):
        phi = 2 * math.pi * k / 30 + rng.uniform(-0.05, 0.05)
        tufts.append(("rim", rng.uniform(0.84, 0.93), phi))
    for phi in (0.3, 1.9, 3.6):
        tufts.append(("top", 0.08, phi))

    for kind, fraction, phi in tufts:
        theta = hair_edge(phi) * fraction
        base = hair_point(theta, phi, 0.97)
        normal = (base - HAIR_CENTER).normalized()
        down = (hair_point(theta + 0.03, phi, 0.97) - base).normalized()
        around = Vector((-math.sin(phi), math.cos(phi), 0))
        front = -math.sin(phi)
        length = rng.uniform(0.042, 0.056)
        radius = rng.uniform(0.025, 0.031)
        swing = around * rng.uniform(-0.3, 0.3)
        if kind == "top":
            direction = (UP * 0.8 + down * 0.5 + swing).normalized()
            bend = -down * 0.4 + UP * 0.2
            length = 0.05
            radius = 0.02
        elif kind == "crown":
            direction = (normal * 0.7 + down * 0.5 + swing).normalized()
            bend = normal * 0.4 + UP * 0.2
            length *= 0.8
        elif kind == "middle":
            direction = (down * 0.8 + normal * 0.3 + swing).normalized()
            bend = normal * 0.5 + UP * 0.1
        elif front > 0.6:
            direction = (down * 0.9 + normal * 0.2 + swing).normalized()
            bend = normal * 0.45 + UP * 0.15
            length *= 0.68
        elif front < -0.55:
            direction = (down * 0.9 + normal * 0.15 + swing * 0.5).normalized()
            bend = normal * 0.4 + UP * 0.2
        else:
            direction = (down * 0.75 + normal * 0.35 + swing).normalized()
            bend = normal * 0.6 + UP * rng.uniform(0.4, 0.7)
            length *= 1.05
        start = base - normal * 0.014
        points = [start,
                  base + direction * length * 0.3,
                  base + direction * length * 0.62 + bend * length * 0.12,
                  base + direction * length * 0.9 + bend * length * 0.35]
        tip = points[-1] + (direction + bend * 0.7).normalized() * length * 0.16
        tube(part, mats["hair"], head, points,
             [(radius, radius * 0.75), (radius * 0.84, radius * 0.66),
              (radius * 0.5, radius * 0.42), (radius * 0.2, radius * 0.17)],
             segments=6, side=lambda p, around=around: around, cap_start="flat", cap_end=tip)


def build_scarf(part, mats):
    neck_weights = envelope("chest", "neck")
    loop = []
    for i in range(24):
        a = 2 * math.pi * i / 24
        front = max(0.0, -math.sin(a))
        loop.append(Vector((0.05 * math.cos(a), 0.047 * math.sin(a) + 0.004, 0.706 - 0.012 * front)))
    tube(part, mats["scarf"], neck_weights, loop, [(0.016, 0.02)] * 24, segments=10,
         side=lambda p: (p - Vector((0, 0.004, p.z))), closed=True)
    ellipsoid(part, mats["scarf"], neck_weights, (0.024, -0.052, 0.688), (0.019, 0.014, 0.018),
              rotation=Euler((0.3, 0, 0.5)).to_matrix(), segments=12, rings=8)
    front_tail = [(0.024, -0.058, 0.68), (0.028, -0.075, 0.645), (0.034, -0.084, 0.6),
                  (0.04, -0.087, 0.558)]
    tube(part, mats["scarf"], envelope("chest", "scarf_front"), front_tail,
         [(0.019, 0.006), (0.023, 0.006), (0.025, 0.006), (0.026, 0.006)], segments=10,
         round_start=True, round_end=True, round_length=0.006)
    back_tail = [(0.035, -0.046, 0.7), (0.058, -0.022, 0.716), (0.074, 0.025, 0.715),
                 (0.07, 0.064, 0.69), (0.052, 0.079, 0.625), (0.04, 0.083, 0.56),
                 (0.035, 0.088, 0.5), (0.035, 0.092, 0.435)]
    tube(part, mats["scarf"],
         envelope("chest", "scarf_back.1", "scarf_back.2", "scarf_back.3", "scarf_back.4"),
         back_tail, [(0.02, 0.006), (0.023, 0.006), (0.025, 0.006), (0.026, 0.006),
                     (0.027, 0.006), (0.028, 0.006), (0.028, 0.006), (0.029, 0.006)],
         segments=10, round_start=True, round_end=True, round_length=0.006)


def build_armature(collection):
    data = bpy.data.armatures.new("LittlePrinceRig")
    data.display_type = "OCTAHEDRAL"
    armature = bpy.data.objects.new("LittlePrinceRig", data)
    collection.objects.link(armature)
    armature.show_in_front = True
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.mode_set(mode="EDIT")
    for name, head, tail, parent, connect, deform in all_bones():
        bone = data.edit_bones.new(name)
        bone.head = head
        bone.tail = tail
        bone.roll = 0
        bone.use_deform = deform
        if parent:
            bone.parent = data.edit_bones[parent]
            bone.use_connect = connect
    bpy.ops.object.mode_set(mode="OBJECT")
    body = data.collections.new("Body")
    scarf = data.collections.new("Scarf")
    for bone in data.bones:
        (scarf if bone.name.startswith("scarf") else body).assign(bone)
    return armature


# Animation helpers. Rotations are given in degrees about the armature's axes
# (X: lean forward / swing back, Y: tilt sideways, Z: turn), applied relative
# to the parent bone, so the same numbers mean the same thing for every bone.

def world_rotation(armature, name, degrees):
    rest = armature.data.bones[name].matrix_local.to_3x3()
    rotation = Euler([math.radians(value) for value in degrees], "XYZ").to_matrix()
    return (rest.inverted() @ rotation @ rest).to_quaternion()


def symmetric(pose, name, degrees):
    x, y, z = degrees
    pose[name + ".L"] = (x, y, z)
    pose[name + ".R"] = (x, -y, -z)


def apply_pose(armature, pose, hips=(0, 0, 0)):
    for bone in armature.pose.bones:
        bone.rotation_mode = "QUATERNION"
        bone.rotation_quaternion = (1, 0, 0, 0)
        bone.location = (0, 0, 0)
    for name, degrees in pose.items():
        armature.pose.bones[name].rotation_quaternion = world_rotation(armature, name, degrees)
    rest = armature.data.bones["hips"].matrix_local.to_3x3()
    armature.pose.bones["hips"].location = rest.inverted() @ Vector(hips)


def lowest_foot_point(armature):
    bpy.context.view_layer.update()
    lowest = 10.0
    for side in (1, -1):
        suffix = ".L" if side > 0 else ".R"
        for bone, rest_point in (("foot", (0.062, 0.042, 0.0)), ("foot", (0.064, -0.05, 0.0)),
                                 ("toe", (0.066, -0.1, 0.004))):
            pose_bone = armature.pose.bones[bone + suffix]
            point = Vector((rest_point[0] * side, rest_point[1], rest_point[2]))
            posed = pose_bone.matrix @ pose_bone.bone.matrix_local.inverted() @ point
            lowest = min(lowest, posed.z)
    return lowest


def key_pose(armature, frame):
    for bone in armature.pose.bones:
        bone.keyframe_insert("rotation_quaternion", frame=frame, group=bone.name)
    armature.pose.bones["hips"].keyframe_insert("location", frame=frame, group="hips")


def make_action(armature, name, frames, cyclic=False, asset=False):
    action = bpy.data.actions.new(name)
    action.use_fake_user = True
    animation = armature.animation_data or armature.animation_data_create()
    animation.action = action
    for frame, pose, hips in frames:
        apply_pose(armature, pose, hips)
        key_pose(armature, frame)
    action.use_frame_range = True
    action.frame_start = frames[0][0]
    action.frame_end = max(frames[-1][0], frames[0][0] + 1)
    action.use_cyclic = cyclic
    animation.action = None
    track = animation.nla_tracks.new()
    track.name = name
    track.strips.new(name, int(frames[0][0]), action)
    track.mute = True
    if asset:
        action.asset_mark()
        action.asset_data.description = f"Little Prince pose: {name}"
    return action


def relaxed():
    pose = {}
    symmetric(pose, "upper_arm", (3, 13, 0))
    symmetric(pose, "forearm", (-10, 0, 0))
    symmetric(pose, "hand", (-4, 0, 0))
    return pose


def bump(value, width):
    value = (value + math.pi) % (2 * math.pi) - math.pi
    return math.exp(-(value / width) ** 2)


def gait(phase, run=False):
    stride = 44 if run else 26
    knee_swing = 95 if run else 55
    arm_swing = 34 if run else 17
    pose = relaxed()
    for suffix, offset, side in ((".L", 0.0, 1), (".R", math.pi, -1)):
        phi = phase + offset
        thigh = -stride * math.cos(phi) - (8 if run else 0)
        loading = max(0.0, math.sin(2 * phi)) if math.sin(phi) > 0 else 0.0
        knee = 6 + knee_swing * max(0.0, -math.sin(phi)) ** 1.3 + (14 if run else 8) * loading
        foot_world = -14 * bump(phi, 0.45) + (36 if run else 28) * bump(phi - 0.86 * math.pi, 0.4)
        pose["thigh" + suffix] = (thigh, 0, 0)
        pose["shin" + suffix] = (knee, 0, 0)
        pose["foot" + suffix] = (foot_world - thigh - knee, 0, 0)
        pose["toe" + suffix] = (-max(0.0, foot_world) * 0.8, 0, 0)
        upper = arm_swing * math.cos(phi)
        pose["upper_arm" + suffix] = (upper + 3, 13 * side, 0)
        pose["forearm" + suffix] = (-(45 if run else 12) - (25 if run else 10) * max(0.0, -math.cos(phi)), 0, 0)
    lean = 9 if run else 3
    pose["hips"] = (lean * 0.4, 2 * math.sin(phase), -5 * math.cos(phase))
    pose["spine"] = (lean * 0.4, 0, 3 * math.cos(phase))
    pose["chest"] = (lean * 0.3, -1.5 * math.sin(phase), 5 * math.cos(phase))
    pose["neck"] = (-lean * 0.4, 0, 0)
    pose["head"] = (-lean * 0.3 + 1.5 * math.cos(2 * phase), 1.2 * math.sin(phase), -2 * math.cos(phase))
    lift = 34 if run else 3
    for index in range(1, 5):
        lag = 1.0 + 0.7 * index
        pose[f"scarf_back.{index}"] = (lift / index + (6 if run else 3) * math.sin(2 * phase - lag),
                                      3 * math.sin(phase - lag), 0)
    pose["scarf_front"] = ((22 if run else 3) + 3 * math.sin(2 * phase - 1.2), 0, 3 * math.sin(phase))
    return pose


def gait_frames(armature, frames, run=False):
    result = []
    for frame in range(frames + 1):
        phase = 2 * math.pi * frame / frames
        pose = gait(phase, run)
        sway = 0.006 * math.sin(phase)
        apply_pose(armature, pose)
        lift = -lowest_foot_point(armature)
        if run:
            lift += 0.018 * max(0.0, math.sin(2 * phase - 0.9))
        result.append((frame + 1, pose, (sway, 0, lift)))
    result[-1] = (frames + 1, result[0][1], result[0][2])
    return result


def idle_pose(phase):
    pose = relaxed()
    breath = math.sin(phase)
    pose["spine"] = (0.8 * breath, 0, 0)
    pose["chest"] = (-1.2 * breath, 0, 0)
    pose["shoulder.L"] = (0, -1.2 * breath, 0)
    pose["shoulder.R"] = (0, 1.2 * breath, 0)
    pose["head"] = (1.5 * math.sin(phase + 0.5), 3 * math.sin(phase * 0.5 + 0.3), 4 * math.sin(phase * 0.5))
    pose["upper_arm.L"] = (3 + 1.5 * math.sin(phase + 0.4), 13, 0)
    pose["upper_arm.R"] = (3 + 1.5 * math.sin(phase + 0.9), -13, 0)
    for index in range(1, 5):
        pose[f"scarf_back.{index}"] = (3 * math.sin(phase - 0.6 * index), 2 * math.sin(phase * 0.5 - 0.5 * index), 0)
    pose["scarf_front"] = (2 * math.sin(phase - 0.5), 0, 0)
    return pose


def wave_pose(phase):
    pose = relaxed()
    pose["upper_arm.R"] = (-12, 98, 0)
    pose["forearm.R"] = (-14, 22 + 24 * math.sin(phase), 0)
    pose["hand.R"] = (0, 14 * math.sin(phase - 0.7), 0)
    pose["chest"] = (0, -3, -4)
    pose["head"] = (2, 7, -5)
    for index in range(1, 5):
        pose[f"scarf_back.{index}"] = (2 * math.sin(phase - 0.6 * index), 0, 0)
    return pose


def stargaze_pose():
    pose = relaxed()
    pose["spine"] = (-3, 0, 0)
    pose["chest"] = (-6, 0, 0)
    pose["neck"] = (-14, 0, 0)
    pose["head"] = (-24, 6, 0)
    symmetric(pose, "upper_arm", (14, 15, 0))
    symmetric(pose, "forearm", (-6, 0, 0))
    for index in range(1, 5):
        pose[f"scarf_back.{index}"] = (36 if index == 1 else 14, 6, 0)
    pose["scarf_front"] = (18, 0, 8)
    return pose


def leg_angles(hip_z, ankle_target):
    """Two-bone solve in the side view: returns thigh, shin and foot X rotations."""
    hip, knee = SEGMENTS["thigh.L"]
    _, ankle = SEGMENTS["shin.L"]
    thigh_length = (knee - hip).length
    shin_length = (ankle - knee).length

    def angle(vector):
        return math.atan2(vector.y, -vector.z)

    hip_posed = Vector((0, hip.y, hip_z))
    target = Vector((0, ankle_target[0], ankle_target[1]))
    line = target - hip_posed
    distance = min(line.length, thigh_length + shin_length - 1e-4)
    alpha = math.acos((thigh_length ** 2 + distance ** 2 - shin_length ** 2) / (2 * thigh_length * distance))
    thigh_world = angle(line) - alpha
    knee_posed = hip_posed + Vector((0, math.sin(thigh_world), -math.cos(thigh_world))) * thigh_length
    shin_world = angle(target - knee_posed)
    thigh_delta = thigh_world - angle(knee - hip)
    shin_delta = shin_world - angle(ankle - knee) - thigh_delta
    return math.degrees(thigh_delta), math.degrees(shin_delta), -math.degrees(thigh_delta + shin_delta)


def sit_pose():
    pose = relaxed()
    drop = -0.392
    hip_z = SEGMENTS["thigh.L"][0].z + drop
    thigh, shin, foot = leg_angles(hip_z, (-0.25, 0.066))
    symmetric(pose, "thigh", (thigh, 0, 4))
    symmetric(pose, "shin", (shin, 0, 0))
    symmetric(pose, "foot", (foot, 0, 0))
    pose["hips"] = (-6, 0, 0)
    pose["spine"] = (10, 0, 0)
    pose["chest"] = (10, 0, 0)
    pose["neck"] = (-6, 0, 0)
    pose["head"] = (-10, -5, 6)
    symmetric(pose, "upper_arm", (-40, 10, 0))
    symmetric(pose, "forearm", (-30, -8, 0))
    symmetric(pose, "hand", (10, 0, 0))
    for index in range(1, 5):
        pose[f"scarf_back.{index}"] = (-10 if index == 1 else 8, 0, 0)
    pose["scarf_front"] = (-12, 0, 0)
    return pose, (0, 0.03, drop)


def jump_pose(phase):
    pose = relaxed()
    flutter = math.sin(phase)
    for suffix, side, tuck in ((".L", 1, 1.0), (".R", -1, 0.55)):
        thigh = -38 * tuck - 6 * math.sin(phase + side)
        shin = 30 + 45 * tuck + 6 * math.sin(phase + side)
        pose["thigh" + suffix] = (thigh, 0, 0)
        pose["shin" + suffix] = (shin, 0, 0)
        pose["foot" + suffix] = (22 - thigh - shin, 0, 0)
        pose["upper_arm" + suffix] = (-12 + 4 * flutter, -side * (70 + 6 * flutter), 0)
        pose["forearm" + suffix] = (-25, 0, 0)
        pose["hand" + suffix] = (0, -side * 10, 0)
    pose["spine"] = (4, 0, 0)
    pose["chest"] = (-3, 0, 0)
    pose["head"] = (-8 + 2 * flutter, 0, 0)
    for index in range(1, 5):
        pose[f"scarf_back.{index}"] = ((50 if index == 1 else 12) + 8 * math.sin(2 * phase - index), 0, 0)
    pose["scarf_front"] = (30 + 6 * math.sin(2 * phase - 1), 0, 0)
    return pose


def crouch_pose(amount):
    """Knees bent by `amount` (0 standing, 1 deepest) with the feet kept on the ground."""
    pose = relaxed()
    drop = -0.085 * amount
    hip, _ = SEGMENTS["thigh.L"]
    _, ankle = SEGMENTS["shin.L"]
    thigh, shin, foot = leg_angles(hip.z + drop, (ankle.y, ankle.z))
    symmetric(pose, "thigh", (thigh, 0, 0))
    symmetric(pose, "shin", (shin, 0, 0))
    symmetric(pose, "foot", (foot, 0, 0))
    pose["spine"] = (14 * amount, 0, 0)
    pose["chest"] = (8 * amount, 0, 0)
    pose["neck"] = (-10 * amount, 0, 0)
    pose["head"] = (-8 * amount, 0, 0)
    symmetric(pose, "upper_arm", (3 - 30 * amount, 13 - 45 * amount, 0))
    symmetric(pose, "forearm", (-10 - 30 * amount, 0, 0))
    for index in range(1, 5):
        pose[f"scarf_back.{index}"] = ((30 if index == 1 else 6) * amount, 0, 0)
    pose["scarf_front"] = (16 * amount, 0, 0)
    return pose, (0, 0, drop)


def land_frames():
    frames = []
    for frame in range(21):
        t = frame / 20
        amount = 1 - smoothstep(0.0, 1.0, t) if t > 0.12 else 0.75 + 0.25 * t / 0.12
        pose, hips = crouch_pose(amount)
        frames.append((frame + 1, pose, hips))
    return frames


def cheer_frames():
    frames = []
    for frame in range(51):
        phase = 2 * math.pi * frame / 25
        hop = max(0.0, math.sin(phase))
        squat = max(0.0, -math.sin(phase)) * 0.35
        pose, hips = crouch_pose(squat)
        for suffix, side in ((".L", 1), (".R", -1)):
            pump = math.sin(phase + 0.6 + (0.5 if side > 0 else 0))
            pose["upper_arm" + suffix] = (-10, -side * (128 + 14 * pump), 0)
            pose["forearm" + suffix] = (-6, -side * (12 + 12 * pump), 0)
            pose["hand" + suffix] = (0, -side * 10 * pump, 0)
        pose["head"] = (-10 + 4 * hop, 4 * math.sin(phase / 2), 0)
        for index in range(1, 5):
            pose[f"scarf_back.{index}"] = (16 * hop + 4 * math.sin(phase - index), 0, 0)
        frames.append((frame + 1, pose, (hips[0], hips[1], hips[2] + 0.035 * hop)))
    frames[-1] = (51, frames[0][1], frames[0][2])
    return frames


def build_actions(armature):
    idle = [(1 + i * 3, idle_pose(4 * math.pi * i / 48), (0, 0, 0)) for i in range(49)]
    make_action(armature, "Idle", idle, cyclic=True)
    make_action(armature, "Walk", gait_frames(armature, 32), cyclic=True)
    make_action(armature, "Run", gait_frames(armature, 24, run=True), cyclic=True)
    jump = [(1 + i * 4, jump_pose(2 * math.pi * i / 8), (0, 0, 0)) for i in range(9)]
    make_action(armature, "Jump", jump, cyclic=True)
    make_action(armature, "Land", land_frames())
    make_action(armature, "Cheer", cheer_frames(), cyclic=True)
    wave = [(1 + i * 3, wave_pose(2 * math.pi * i / 12), (0, 0, 0)) for i in range(13)]
    make_action(armature, "Wave", wave, cyclic=True)
    make_action(armature, "Stand", [(1, relaxed(), (0, 0, 0))], asset=True)
    make_action(armature, "Wave Pose", [(1, wave_pose(math.pi / 2), (0, 0, 0))], asset=True)
    make_action(armature, "Stargaze", [(1, stargaze_pose(), (0, 0, 0))], asset=True)
    pose, hips = sit_pose()
    make_action(armature, "Sit", [(1, pose, hips)], asset=True)
    apply_pose(armature, {})


def build_stage(collection):
    world = bpy.data.worlds.new("Paper")
    world.use_nodes = True
    background = world.node_tree.nodes["Background"]
    background.inputs["Color"].default_value = (*srgb("#F2E8D2"), 1)
    background.inputs["Strength"].default_value = 0.9
    bpy.context.scene.world = world

    bpy.ops.mesh.primitive_uv_sphere_add(segments=128, ring_count=64, radius=3.0, location=(0, 0, -3.0))
    planet = bpy.context.object
    planet.name = "Asteroid"
    planet.data.materials.append(material("Asteroid", "#B3ABC2", roughness=0.9))
    bpy.ops.object.shade_smooth()
    for user in list(planet.users_collection):
        user.objects.unlink(planet)
    collection.objects.link(planet)

    target = bpy.data.objects.new("CameraTarget", None)
    target.location = (0, 0, 0.5)
    collection.objects.link(target)
    camera_data = bpy.data.cameras.new("Camera")
    camera_data.lens = 70
    camera = bpy.data.objects.new("Camera", camera_data)
    camera.location = (1.35, -3.2, 0.95)
    constraint = camera.constraints.new("TRACK_TO")
    constraint.target = target
    collection.objects.link(camera)
    bpy.context.scene.camera = camera

    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun_data.energy = 3.2
    sun_data.angle = math.radians(12)
    sun_data.color = srgb("#FFF1DC")
    sun = bpy.data.objects.new("Sun", sun_data)
    sun.rotation_euler = Euler((math.radians(50), 0, math.radians(-35)))
    collection.objects.link(sun)
    rim_data = bpy.data.lights.new("Rim", "AREA")
    rim_data.energy = 60
    rim_data.size = 2
    rim_data.color = srgb("#D8E4FF")
    rim = bpy.data.objects.new("Rim", rim_data)
    rim.location = (-1.2, 2.0, 1.6)
    rim.constraints.new("TRACK_TO").target = target
    collection.objects.link(rim)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    scene.frame_start = 1
    scene.frame_end = 33
    scene.view_settings.view_transform = "Standard"
    export = bpy.data.collections.new("Export")
    stage = bpy.data.collections.new("Stage")
    scene.collection.children.link(export)
    scene.collection.children.link(stage)

    mats = {
        "skin": material("Skin", "#F7D6BE", 0.55, Subsurface_Weight=0.08),
        "cheek": material("Cheek", "#F2A3A0", 0.6),
        "eye": material("Eye", "#2A211E", 0.25),
        "brow": material("Brow", "#E0B354", 0.7),
        "mouth": material("Mouth", "#B9645C", 0.6),
        "hair": material("Hair", "#F4CF55", 0.55, Sheen_Weight=0.25),
        "suit": material("Suit", "#97CBA8", 0.75, Sheen_Weight=0.3),
        "belt": material("Belt", "#7B5434", 0.6),
        "buckle": material("Buckle", "#D8B25A", 0.35, Metallic=0.7),
        "button": material("Button", "#D8B25A", 0.35, Metallic=0.7),
        "scarf": material("Scarf", "#F2B630", 0.8, Sheen_Weight=0.4),
        "boot": material("Boot", "#8A5B3B", 0.5),
    }

    armature = build_armature(export)
    builders = [("Suit", build_suit, 0.55), ("Boots", build_boots, 0.5), ("Hands", build_hand, 0.3),
                ("Head", build_head, 0.45), ("Hair", build_hair, 0.3), ("Scarf", build_scarf, 0.45)]
    for name, builder, game_detail in builders:
        part = Part(name)
        builder(part, mats)
        obj = part.build(export, armature)
        obj["game_detail"] = game_detail

    build_actions(armature)
    build_stage(stage)
    layers = bpy.context.view_layer.layer_collection
    bpy.context.view_layer.active_layer_collection = layers.children["Export"]

    depsgraph = bpy.context.evaluated_depsgraph_get()
    triangles = 0
    for obj in export.objects:
        if obj.type == "MESH":
            evaluated = obj.evaluated_get(depsgraph)
            mesh = evaluated.to_mesh()
            mesh.calc_loop_triangles()
            print(f"{obj.name}: {len(mesh.loop_triangles)} triangles")
            triangles += len(mesh.loop_triangles)
            evaluated.to_mesh_clear()
    print(f"triangles in the viewport: {triangles}")
    bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT), compress=True)
    print(f"saved {OUTPUT}")


main()
