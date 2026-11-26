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
   (B612, then the lamplighter's planet, then the desert).
2. **Arrival, conversation, task.** Each planet has a character to talk to
   (the rose, the lamplighter, the fox and the aviator) and a main task. Side
   goals give stardust and achievements.
3. **Ending.** With all three tasks done the birds carry the traveller home to
   B612 for a gentle ending: the rose, a sunset, and the aviator looking up at
   the stars on Earth. Afterwards the game continues in free play.

## Travellers

Four playable travellers from KayKit Adventurers 1.0 (CC0), each wearing the
prince's scarf. Every planet can be finished with any of them; each is better
at one kind of trouble.

| Traveller | Strength |
| --- | --- |
| Hooded rogue | Fast, and moves without startling animals |
| Knight | A shield against volcano cinders; pushes rocks aside |
| Mage | A light that drives off snuffer moths |
| Barbarian | Jumps high; landing stuns small troubles nearby |

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
| Y | Swap traveller | |
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
- Four neighbouring cells that are all far away are drawn as one mesh, so a
  view from high above does not cost one draw call per cell.
- The sky backdrop and stars are drawn just beyond the farthest ground the
  camera can see, and no nearer than 26 units, so they never cover the planet
  while keeping the 16-bit depth buffer precise.

## Milestones

| # | Content |
| --- | --- |
| M1 | This document; bigger planets |
| M2 | Saves, settings, pause menu, controls, statistics |
| M3 | Star map, conversations, interaction, tasks |
| M4 | Troubles and stardust; B612's task |
| M5 | Four travellers |
| M6 | The lamplighter's and the desert's tasks |
| M7 | Journal, achievements, ending, free play, texts and documentation |

Each milestone is a pull request; `LITTLEPRINCE_SCRIPT` (added with the first
task) replays button presses to finish a planet's task and the whole story,
and runs as a test.

## Not planned

- New planets (the king, the vain man, the businessman, the geographer).
- Combat, health or failure.
- Quoting translations of the book; texts are written afresh.
