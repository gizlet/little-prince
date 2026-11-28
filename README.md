# The Little Prince · 小王子

A 3D game for [Gizlet OS](https://github.com/gizlet/os) handhelds such as the R36S, written in Rust with raylib. A small hooded traveller walks three small planets, each lit by its own sun and turning through day, sunset and night, and migrating birds carry it from one planet to the next. It shows what a 1.3 GHz quad-core Cortex-A35 with a Mali-G31 can draw on a 640×480 screen, and grows a little at a time: [docs/design.md](docs/design.md) describes the adventure it is becoming.

![B612](docs/screenshots/b612.png)

| Title | The lamplighter's planet | The desert |
| --- | --- | --- |
| ![Title](docs/screenshots/title.png) | ![Lamplighter](docs/screenshots/lamp.png) | ![Desert](docs/screenshots/desert.png) |

| The journeys | The star chart | A chapter |
| --- | --- | --- |
| ![Journeys](docs/screenshots/journeys.png) | ![Star chart](docs/screenshots/starmap.png) | ![Chapter](docs/screenshots/chapter.png) |

| B612's troubles | The night watch | A sandstorm |
| --- | --- | --- |
| ![Troubles on B612](docs/screenshots/troubles.png) | ![The night watch](docs/screenshots/watch.png) | ![A sandstorm](docs/screenshots/storm.png) |

## Credits

- **The story** is Antoine de Saint-Exupéry's *Le Petit Prince* (1943). The planets follow the book: B612 with its volcanoes, rose and baobab sprouts, the lamplighter's planet, and the desert where the fox is tamed. The book is in the public domain in most countries but not everywhere, and "Le Petit Prince" is a registered trademark. Keep this demo non-commercial.
- **The hero** is the little prince from [gizlet/blender-models](https://github.com/gizlet/blender-models), our own rigged and animated model (CC0, [license](assets/hero/LICENSE.txt)). `assets/hero/little-prince.glb` is its game export; edit the `.blend` there and export it again with `scripts/build.py -- little-prince --game`. The title screen and a corner credit during play name it.
- **The lamplighter and the fox** are Quaternius's CC0 "Worker" and "Fox" ([license and sources](assets/npc/LICENSE.txt)), recoloured and converted by `scripts/npc.py`. Thank you!
- **Everything else** is written from scratch and made in code at start-up: the planets, grass, flowers, trees, rocks, the biplane, the birds, the troubles, particles, shaders and sound effects.

## What it shows

| | |
| --- | --- |
| B612 | A planet 18 units in radius with rolling hills, a dirt path, 30,000 blades of grass parting around the hero, flowers, bushes, mossy rocks, three baobabs, smoking volcanoes, the rose under its glass, clouds and butterflies. Pull up the spreading baobab sprouts, sweep the volcanoes and turn the caterpillars into butterflies. |
| The lamplighter's planet | The smallest, 15 units in radius, stony and bare, where a day lasts 14 seconds. The lamplighter lights his lamp at dusk and puts it out at dawn. At night snuffer moths flutter in to put it out. |
| The desert | The largest, 25 units in radius: dunes, a field of wheat and a crashed biplane. Sandstorms blow in every minute or so, hiding the way and raising dust devils. Tame the fox, who then follows the prince and digs up two of the aviator's four lost parts. |
| The hero | The little prince's 27-bone rig with idle, walk, run, jump, landing and cheer, blended into each other and timed to the hero's speed so the feet do not slide, skinned on the CPU. |
| The look | Toon lighting with soft shadow colours, ink outlines, a sun that circles each planet, stars and shooting stars, fog, bloom and a paper texture at the high effects level. |
| The HUD | FPS with a frame-time graph, whole-machine and own CPU use, free and total memory, the app's memory, SoC temperature and CPU clock. |

## Play on a Mac

Requirements: [rustup](https://rustup.rs), CMake (raylib-sys builds raylib for the desktop) and [Git LFS](https://git-lfs.com) for the model, fonts and images.

```sh
git lfs install
git clone https://github.com/gizlet/little-prince.git
cd little-prince
cargo run --release
```

| Keys | Button | Action |
| --- | --- | --- |
| Arrows or WASD | D-pad / stick | Walk |
| Space, Enter or K | A | Jump; talk to someone nearby; on the title, open the book; in menus, choose |
| I | X | Open the book at the star chart |
| Q / E, R / F | Right stick | Turn the camera left and right, up and down |
| U | Y | Effects: high, mid, low |
| Tab | Select | Statistics |
| Esc or P | Start | Pause menu: resume, settings, statistics, save and return to the title; settings on the title |
| Backspace or J | B | Back, in menus |

The title, the journeys and the star map are pages of one storybook floating in the night. On the title its cover turns slowly; A opens it at the three journeys, each with its own bookmark: choose one to continue it, or an empty one to start; X twice moves the chosen journey aside (to `slotN.txt.old`). The game saves on arriving at a planet, on scoring, when the pause menu opens, on returning to the title and on quitting, and every minute. The settings hold the language, effects level, performance panel, volume and inverted camera, for all journeys.

Each planet has someone to talk to and a task: on B612 the rose asks for ten baobab sprouts to be pulled up, both volcanoes to be swept (stand beside one for a moment) and the three caterpillars crawling towards her to be jumped on, which turns them into butterflies; the lamplighter wants company through three sunsets and six snuffer moths shooed from his lamp (jump beside one), and in the desert the fox is tamed by standing still while it comes closer, then leads the way to the buried aeroplane parts; all four are to be picked up between sandstorms. Walk up to them and press A; A moves the conversation on and B leaves it.

Nothing in the game hurts the prince. Troubles left alone grow: sprouts left standing seed new ones, and an unswept volcano throws cinders, whose landing spots show as dark shadows first. Walking into a caterpillar or a cinder knocks him back and spills up to three of the stardust he carries, which glitters on the ground for a few seconds to be gathered again. Stardust floats over every planet, comes back a while after it is gathered, and is set free by every trouble seen to; the HUD shows how much he carries. X opens the book at the star chart: the planets drawn in colour once visited and as blue ink sketches before, joined by the birds' dotted routes. Left and right choose a planet and A turns to its chapter, with its story, its task and A to fly there; planets already visited can be flown to at any time, and the route to the next planet opens when the current planet's task is done. B turns back and X closes the book. Pages turn with a curling leaf, and cut straight over at effects level low.

Left alone on the title for 12 seconds, it plays by itself (autoplay) and moves on to the next planet every 26 seconds. Hold Select + Start for two seconds to quit.

| Variable | Effect |
| --- | --- |
| `GIZLET_LANG=en` | Start in English, unless a language was chosen in the settings |
| `GIZLET_SAVE_DIR=DIR` | Where journeys (`slot1.txt` to `slot3.txt`) and `settings.txt` are kept; `./save` by default. Gizlet OS sets it for each app |
| `GIZLET_SWAP_AB=1` | Swap A and B |
| `LITTLEPRINCE_WINDOW=WxH` | Desktop window size (1280x960 by default) |
| `LITTLEPRINCE_AUTOPLAY=1` | Start autoplay at once |
| `LITTLEPRINCE_MUTE=1` | No sound |
| `LITTLEPRINCE_FX=0\|1\|2` | Effects level (2 on the desktop, 1 on the handheld) |
| `LITTLEPRINCE_SCALE=0.8` | Render the 3D scene smaller and scale it up |
| `LITTLEPRINCE_STATS=0` | Hide the stats panel |
| `LITTLEPRINCE_HERO=Name` | Load `assets/hero/Name.glb` instead of `little-prince` (one skin of at most 64 bones, UVs into one texture, and animations named `Idle`, `Walk`, `Run`, `Jump`, `Land` and `Cheer`, or KayKit Adventurers 1.0's names) |
| `LITTLEPRINCE_PROFILE=1` | Print the frame time per stage (waiting for the GPU after each) and the triangles drawn, every 3 seconds; `2` prints only the frame time |
| `LITTLEPRINCE_SKIP=hero,heroink,skin,world,grass,props,flowers,troubles,backdrop,sky,alpha,glass,glow,bloom` | Leave parts out, for measuring their cost |
| `LITTLEPRINCE_FPS=60` | Frame rate cap; `0` leaves the pace to the display |
| `LITTLEPRINCE_SECONDS=80` | Quit after that many seconds, for scripted runs |
| `LITTLEPRINCE_CAPTURE=FILE.png` | Render one frame to a PNG and exit, saving nothing; `LITTLEPRINCE_SCENE` is `title` (or `cover`), `b612`, `lamp`, `desert`, `fly`, `journeys`, `pause`, `settings`, `statistics`, `starmap`, `chapter`, `turn` (a page halfway over), `dialog`, `troubles` (a volcano throwing cinders, with stardust in hand), `watch` (moths at the lamp), `storm` (a sandstorm), `keeper` and `foxclose` (close-ups), `model` or `icon`, and `LITTLEPRINCE_AT` the seconds of autoplay before it |
| `LITTLEPRINCE_SCRIPT=FILE` | Replay the button presses in FILE at 60 steps a second, checking the game's state as it goes; exits with status 1 if a check fails. `assist SECS` lets the game play the planet's task for that long (format in `src/script.rs`) |

```sh
cargo test
scripts/play-tests.sh
```

`scripts/play-tests.sh` plays the button replays in `tests/scripts` on the desktop build, each group of them on a fresh save folder: starting a journey, coming back to it, deleting it, and finishing the three planets' tasks in turn.

## Play on Windows

Install it from the Store in [Gizlet Launcher for Windows](https://github.com/gizlet/windows), which keeps each profile's journeys apart in `GIZLET_SAVE_DIR`. The Windows x64 package is built by `scripts/package.ps1` on Windows, with Python 3.11 or newer, the Rust `x86_64-pc-windows-msvc` target and Visual Studio's C++ tools, CMake and LLVM:

```powershell
./scripts/package.ps1
```

It writes `dist/io.gizlet.littleprince-<version>-x86_64-pc-windows-msvc-full.gzl`, which the Launcher can also import. The C runtime is linked statically (`.cargo/config.toml`), so the game needs nothing else installed.

## Run on Gizlet OS

On the device the game links Gizlet runtime 1's `libraylib.so.600`, built for DRM with OpenGL ES 3.0 (ES 2 runtimes work too; the shaders follow the context raylib makes), and reads the handheld's buttons from evdev. `scripts/package.sh` builds the application and writes its `.gzl` package.

Install Python 3.11 or newer, [zig](https://ziglang.org), and the Rust `aarch64-unknown-linux-gnu` target. Build runtime 1's `libraylib.so` in [gizlet/os](https://github.com/gizlet/os) with `bazel build --config=r36s //runtime:raylib`.

```sh
GIZLET_RAYLIB_LIB=/path/to/folder/with/libraylib.so scripts/package.sh
scp dist/io.gizlet.littleprince-0.3.0-aarch64-unknown-linux-gnu-lite.gzl root@DEVICE:/tmp/
ssh root@DEVICE 'gizlet install /tmp/io.gizlet.littleprince-0.3.0-aarch64-unknown-linux-gnu-lite.gzl'
```

Open the game from the launcher. To publish a release, update the version in
`gizlet.toml` and merge to `main`.
After tests and the `.gzl` build pass, CI creates `v<version>`, publishes the
GitHub Release, and waits for the Gizlet Store checks. Rerun the original CI
run to retry a failed release. Pushing a matching tag or selecting it when
dispatching CI also starts a release.

### Performance on the R36S

On a genuine R36S (RK3326, Mali-G31 through Panfrost, Mesa 25.0) version 0.2's B612 ran at about 50 fps at the default (mid) effects level, with about 45,000 triangles in about 50 draw calls a frame and about 140 MB of memory; with the bigger planets the desktop draws about as many triangles and draw calls while walking. The main thread spends about 14 ms of CPU time per frame, so the GPU sets the pace: its fragment stage is busy about 85% of the time. The high effects level (bloom, paper, the hero's outline) runs at about 36 fps.

On this GPU every triangle costs fragment time, however few pixels it covers: drawing the hero at a quarter of its size saves only about a third of its cost, and drawing half its triangles saves half. So the hero's outline uses a reduced copy of its mesh (about 1,800 triangles instead of 4,000, merged on a grid over the same skinned vertices), and planet cells more than 3 units from the camera carry every other blade of grass. Swapping the hero's shader for a flat one, mipmapping its texture or discarding depth buffers at the end of each pass made no measurable difference.

The biggest single win was giving raylib's render batch 48 buffers in rotation instead of one (`rlLoadRenderBatch`, set with `LITTLEPRINCE_BATCH`). With one buffer, every batch flush rewrites a vertex buffer the GPU is still reading, and Panfrost makes the CPU wait for the GPU each time: about 25 ms a frame went to those waits, and the game ran at 22 fps. The hero's skinned mesh is double-buffered for the same reason.

Other changes that helped: batching small character parts into a few draw calls, indexed meshes, cheaper small parts and far-away scenery, lighting scenery per vertex without rim light, splitting each planet into 80 cells that hold their ground, scenery, flowers and grass in one mesh each (in full nearby, coarser farther off, ground only beyond; 320 cells since the planets grew, whose levels share one vertex and one index buffer, and groups of four cells drawn as one beyond 9 units, with coarser ground beyond 10), drawn nearest first and only when in front of the camera and above the horizon, skinning the hero on the CPU, drawing straight to the screen below the high effects level, drawing opaque things without blending, the sky after the solids, and a HUD redrawn four times a second and blitted only where it holds something.

`LITTLEPRINCE_PROFILE=2` prints the frame time, the triangles, the peak triangles and draw calls in each three-second window, the CPU time before the frame is handed over (`cpu`) and the main thread's CPU time (`thread-cpu`); `1` also splits it into stages, waiting for the GPU after each; `3` splits it without waiting. `LITTLEPRINCE_SKIP` leaves parts out to measure their cost; reading the GPU busy time from `/proc/PID/fdinfo` (with `/sys/bus/platform/drivers/panfrost/*/profiling` set to 1) shows what each part costs the GPU.

The sky backdrop and the stars are drawn just beyond the farthest ground the camera can see and no nearer than 26 units, well inside the far plane: on the handheld's depth buffer things near the far plane can fail the depth test and the sky flickers black.

Do not pin the GPU clock by hand: setting `min_freq` of `/sys/class/devfreq/ff400000.gpu` to its top frequency made the GPU power domain fail (`failed to get ack on domain 'gpu'`) and the screen went black until a reboot. The default `simple_ondemand` governor already reaches 560 MHz under load.

## Layout

| Path | Holds |
| --- | --- |
| `gizlet.toml` | The app manifest |
| `src/main.rs` | The window, the frame loop, captures |
| `src/app.rs` | Modes, the follow camera, autoplay, flights between planets, effects from footsteps and landings, the render order |
| `docs/design.md` | The game design and its milestones |
| `src/world.rs` | The three planets: ground, scenery, the rose, sprouts, volcanoes, the lamplighter's lamp, the fox's ways, the biplane, the sandstorm, sky and stars |
| `src/hero.rs` | Loading the hero model, animation blending, bone matrices, CPU skinning |
| `src/player.rs` | Walking on a sphere, jumps, the procedural prince used when no model is found |
| `src/gl.rs` | Shaders, materials, batching, GPU skinning, outlines, post-processing |
| `src/mesh.rs` | Mesh building, planets, grass, cells |
| `src/fx.rs` | Particles and ripples |
| `src/sound.rs` | Synthesized sound effects |
| `src/hud.rs`, `src/text.rs` | The HUD and menus, strings in Chinese and English, font atlases |
| `src/dialog.rs` | Conversations and their lines |
| `src/trouble.rs` | Stardust, caterpillars, cinders, snuffer moths, dust devils and the aeroplane parts |
| `src/book.rs` | The storybook: its cover, the journeys, the star chart and the chapters, paper made at start-up, page turns and the planets in miniature |
| `src/save.rs` | Journeys and settings on disk |
| `src/menu.rs` | The journeys, pause, settings, statistics, star chart and chapter pages and what their buttons do |
| `src/script.rs`, `tests/scripts`, `scripts/play-tests.sh` | Button replays and the ones played as tests |
| `src/stats.rs` | CPU, memory, temperature and clock readings |
| `src/prof.rs` | The frame profiler |
| `src/input.rs`, `src/evdev.rs` | Keyboard, gamepads and the handheld's evdev devices (from [gizlet/overeaten](https://github.com/gizlet/overeaten)) |
| `scripts/package.sh`, `scripts/pack.py`, `scripts/zig-cc` | The device build and `.gzl` packaging |
| `assets/` | The hero, the lamplighter and the fox, fonts and the icon |
| `scripts/npc.py` | Turns Quaternius's models into the lamplighter and the fox (run with Blender) |

## License

Code under MIT, see [LICENSE](LICENSE). The hero, lamplighter and fox models are CC0 ([hero](assets/hero/LICENSE.txt), [lamplighter and fox](assets/npc/LICENSE.txt)). Fonts: Inter and Space Grotesk under the SIL Open Font License 1.1, Droid Sans Fallback under the Apache License 2.0 ([assets/fonts](assets/fonts)).
