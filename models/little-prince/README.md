# The Little Prince

A stylised, original 3D take on the little prince from Antoine de
Saint-Exupéry's watercolours: tousled golden hair, a pale green suit with a
belt and gold buttons, a yellow scarf and brown boots. He is about one unit
tall, stands on the origin and faces −Y in Blender (+Z in glTF).

![Turnaround](../../docs/little-prince/turnaround.png)
![Actions](../../docs/little-prince/actions.png)
![Poses](../../docs/little-prince/poses.png)
![Walk](../../docs/little-prince/walk.png)

| | Smooth | Game |
| --- | --- | --- |
| Triangles | about 49,000 | about 4,700 |
| Materials | 12 coloured materials | one 64×16 palette texture |
| File | about 1.6 MB | about 0.4 MB |

## Edit and export

Open `little-prince.blend` in Blender 4.5 LTS. The armature and meshes in the
`Export` collection go into the GLB, with the armature's NLA actions as
animations. The `Stage` collection contains the camera and lights for previews.

From the repository root, run this to regenerate `assets/hero/little-prince.glb`:

```sh
blender --background --factory-startup --python-exit-code 1 --python scripts/export-hero.py
```

The export checks animation durations and the game's mesh, skin, and texture
limits before replacing the GLB. Append `-- --smooth` for
`dist/little-prince.glb`, or `-- --subdivision 0` for
`dist/little-prince-sub0.glb`.

To regenerate the preview sheets:

```sh
blender --background models/little-prince/little-prince.blend \
  --python-exit-code 1 --python scripts/preview-hero.py -- docs/little-prince
```

The model and its preview sheets are available under [CC0 1.0](../LICENSE).

## Rig

27 bones with `.L`/`.R` names that Blender mirrors: `root` (not deforming),
`hips`, `spine`, `chest`, `neck`, `head`, and on each side `shoulder`,
`upper_arm`, `forearm`, `hand`, `thigh`, `shin`, `foot` and `toe`. The scarf
has `scarf_front` and a chain `scarf_back.1` to `scarf_back.4` for its
trailing end. Bone collections `Body` and `Scarf` separate them.

## Animations

At 30 frames a second. Looping animations repeat their first frame at the end.

| Action | Frames | Loops | |
| --- | --- | --- | --- |
| `Idle` | 145 | yes | Two breaths while looking around |
| `Walk` | 33 | yes | The planted foot moves at 0.89 heights a second |
| `Run` | 25 | yes | The planted foot moves at about 2.1 heights a second |
| `Jump` | 33 | yes | In the air, knees tucked, arms out, scarf flying |
| `Land` | 21 | no | Lands in a crouch and stands up |
| `Cheer` | 51 | yes | Arms up, hopping |
| `Wave` | 37 | yes | Waves with the right hand |

Engines that play `Walk` or `Run` at a ground speed `v` (in heights a
second) keep the feet from sliding with a playback rate of `v / 0.89` or
`v / 2.1`.

## Poses

One-frame actions marked as assets: `Stand`, `Sit`, `Stargaze` (looking up)
and `Wave Pose`.

## Meshes

Six meshes share the rig: `Suit`, `Boots`, `Hands`, `Head` (with the face),
`Hair` and `Scarf`. Each has a level-1 Subdivision modifier (level 2 for
renders) and a `game_detail` custom property, the fraction of its unsubdivided
triangles the game export keeps.

`generate.py` built the first version of `little-prince.blend` from code:

```sh
"$BLENDER" --background --factory-startup --python models/little-prince/generate.py
```

Running it again overwrites the `.blend`.
