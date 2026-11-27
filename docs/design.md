# Design: a short adventure on three planets

The Little Prince started as a walking demo: three small planets, one quiet
task on each and a flight between them. This document describes where it is
going: a short adventure that can be played from start to end on a handheld,
in sittings of five to ten minutes. It adds no new planets; it makes the three
it has bigger and gives each of them something to do.

## Pillars

- **Patience over speed.** Taming takes time, as in the book: some things are
  done by sitting down and waiting, not by running at them.
- **Nothing dies.** Troubles grow if they are left alone, but they never end
  the game. Touching one knocks the traveller back and spills a little
  stardust.
- **Every planet is a short story.** Each has a conversation, a main task and
  its own kind of trouble, and remembers what was done there.
- **The handheld sets the budget.** New content may not cost more to draw than
  version 0.2 did on the R36S (see [Budget](#budget)).

## The planets

| Planet | Radius | Day | Main task | Trouble |
| --- | --- | --- | --- | --- |
| The lamplighter's | 15 | 14 s | Keep the night watch: light the small lamps before dusk and keep them lit until dawn | Snuffer moths that put lamps out after dark |
| B612 | 18 | 240 s | Keep the planet: pull up baobab sprouts before they spread, keep caterpillars from the rose, sweep the volcanoes | Spreading sprouts, caterpillars, volcanoes that spit cinders when neglected |
| The desert on Earth | 25 | 200 s | Find the aviator's lost parts between sandstorms and tame the fox, who then follows the traveller | Sandstorms that hide the way and push the traveller off course; a snake to keep away from |

The radii follow the book's order: the lamplighter's planet is the smallest,
Earth by far the largest. A day is the time the sun takes to circle a planet;
it does not depend on the radius, so the lamplighter's day still lasts 14
seconds on a planet ten times as wide as before.

Layouts are written in world units, not angles, so the distances between
things stay walkable when a radius changes. Scenery is densest along paths and
round landmarks and thins out over open ground, which keeps the far side of a
planet cheap to draw.

## Structure

1. **Star map.** X opens a map of the three planets joined by the migrating
   birds' routes. Planets already visited can be revisited at any time; a new
   route opens when the previous planet's main task is done
   (B612, then the lamplighter's planet, then the desert). The map is a page
   of the storybook (see below).
2. **Arrival, conversation, task.** Each planet has a character to talk to
   (the rose, the lamplighter, the fox and the aviator) and a main task. Side
   goals give stardust and achievements.
3. **Ending.** With all three tasks done the birds carry the traveller home to
   B612 for a gentle ending: the rose, a sunset, and the aviator looking up at
   the stars on Earth. Afterwards the game continues in free play.

## The prince

There is one hero, the little prince. His model is made in
[gizlet/blender-models](https://github.com/gizlet/blender-models) and its game
export lives in `assets/hero/little-prince.glb`. The hero is loaded from one
file with named animations (`Idle`, `Walk`, `Run`, `Jump`, `Land`, `Cheer`),
so updating the model touches only `assets/hero`, and the stride constants in
`src/hero.rs` if the walk or run changes.

## The storybook

The title, the journeys and the star map are pages of one illustrated book
on a starry background, drawn at 640×480:

- **Cover.** The closed book floats in space and turns slowly, with the
  title on its cover. A opens it.
- **Journeys.** The first spread holds the three journeys as bookmarked
  pages: the planet the journey stopped on, its tasks and play time.
- **Star chart.** A parchment sheet with torn edges and watercolour clouds.
  The three planets are the planets themselves in miniature (their coarse
  ground and scenery in one mesh, simplified to about 5,000 to 12,500
  triangles) turning slowly: in colour once visited, as blue ink sketches
  before that, with a gold ring round the current one. Dotted bird routes
  join them; a locked route is drawn faint. Left and right move between
  planets. The paper is generated at start-up, so the book needs no images.
- **Chapter page.** A on a planet turns the page to its chapter: the planet
  large on the left page; on the right the chapter number, the planet's
  name, one line of story, the task's progress and "Fly there" (or "Stay
  here", or why the route is still closed). A flies, left and right turn to
  the next chapter, B turns back to the chart.
- **Page turns.** A curled page mesh sweeps across the spread in about half a
  second; menus stay usable with no effects (level 0) by cutting instead.

X during play opens the book straight on the star chart, and closes it from
any page. The book is drawn only while it is open, so it costs nothing on a
planet; once it fills the screen the scene behind it is not drawn, and the
chart costs about 23,000 triangles in 3 draw calls, a chapter about 12,000
in one, half of walking.

## Saves and records

- Three save slots in `GIZLET_SAVE_DIR` (`./save` on the desktop), plain text
  lines with a version number, written to a temporary file and renamed into
  place. Unknown lines are kept out of the way so newer saves still load.
- Saves happen on arrival, when a task is done, when the menu opens and on
  quitting. Settings (language, effects level, camera, volume, A/B swap) are a
  separate file shared by all slots.
- A journal kept like the geographer's book: planets, characters and things
  seen, statistics (sunsets watched, sprouts pulled, distance walked, play
  time) and about twenty achievements.

## Controls

| Button | Play | Menus |
| --- | --- | --- |
| D-pad / stick | Walk | Move |
| A | Jump; talk or act when something is near | Choose |
| B | Hold to sit and wait | Back |
| X | Star map | |
| Y | Effects level | |
| Start | Pause menu | |
| Select | Journal | |

## Budget

On a genuine R36S version 0.2 runs at about 50 fps with about 45,000
triangles in about 50 draw calls a frame on B612. Desktop measurements stand in
for the handheld while developing: `LITTLEPRINCE_PROFILE=2` prints the average
triangles a frame and the peak triangles and draw calls in each three-second
window. Walking on any planet should stay within version 0.2's numbers.
Memory is not a budget, but less is better.

To afford planets four to eight times the area:

- Each planet is split into 320 cells. A cell's four levels of detail share
  one vertex buffer and one index buffer: the far level is the ground, the
  near level adds the scenery, flowers and every other blade of grass, the
  close level the rest of the grass; the middle level adds coarse scenery with
  its own index list.
- Four neighbouring cells that are all more than 9 units away are drawn as
  one mesh, so a view from high above or over hills does not cost one draw
  call per cell. Beyond 10 units a group carries only its ground, built
  four times coarser (subdivision 4 instead of 6) with a short skirt hanging
  from its open edges to hide slivers where it meets finer ground. The title
  stretches these distances by 1.6, so its trees stay in view.
- Cells below the horizon are skipped. Each of 16 directions round the camera
  keeps the lowest ground within reach, and a cell is hidden only when a
  sphere that size would hide it: valleys never hide visible ground, and a
  valley behind the camera does not widen the view ahead.
- The sky backdrop and stars are drawn just beyond the farthest ground the
  camera can see, and no nearer than 26 units, so they never cover the planet
  while keeping the 16-bit depth buffer precise.

## Milestones

| # | Content |
| --- | --- |
| M1 | This document; bigger planets |
| M2 | Saves, settings, pause menu, controls, statistics |
| M3 | Routes, conversations, interaction, tasks (the star map as a plain list) |
| M4 | The storybook: cover, journeys, star chart, chapter pages |
| M5 | Troubles and stardust; B612's task |
| M6 | The lamplighter's and the desert's tasks |
| M7 | Journal, achievements, ending, free play, texts and documentation |

Each milestone is a pull request; `LITTLEPRINCE_SCRIPT` (added with the first
task) replays button presses to finish a planet's task and the whole story,
and runs as a test.

## Not planned

- New planets (the king, the vain man, the businessman, the geographer).
- Combat, health or failure.
- Quoting translations of the book; texts are written afresh.
