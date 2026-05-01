# TASKS

## Bugs

## Features

- [ ] Replace the auto fetch on start ON/OFF option with an option based setting, possible options could be to fetch at
  the start (default), fetch every 1 hour/day (check last time it was fetched), and an additional option to manually
  fetch (also write how long it's been since the last fetch next to it).
- [ ] In editor mode, show a warning while deleting a category.
- [ ] Add the same category connector characters from the feed list into the editor side.
- [ ] Make the ghost feed (original of the one being moved) move ghosty, less white.
- [ ] On the right most side of each article, write (in colors) how long ago it was shared.
- [ ] Add a "zen" mode, it switches to article only screen where you can read it much better in a bigger space and not
  have to constantly see FeedList etc.
- [ ] While the cursor is on top options or anything that might be hard to understand, have a little description that
  tells what it is. This is mostly important for options, I don't really know where this text could be so suggest me
  options.

## UI Changes

- [ ] Clean-out the controls, maybe move things to "keybindings" screen (suggest me possible better options).
- [ ] Add more themes (add some popular ones like gruvbox), add a way for me (dev) to add more themes (make it easy to
  add more).
- [ ] If a Feed name can't fit the screen, write it shorter and just put `...` at the end so it looks like
  `My long feed na... [X]` (it already scrolls to show the whole name)
- [ ] Have a visual indicator in scrollable spaces that shows how far down you are etc, literally a scrollbar.
- [ ] `fetched X ago` under `ArticleList` doesn't really look that great, it looks too crowded. Suggest me solutions.

## Debt

- [ ] Clean up the code a little bit, especially in places where you think it might be too crowded and require a split (
  you can create new files and move stuff around however you like).

## Done

- [x] 2026-04-17 - Fix: can't move categories in editor (Space on left panel now starts category move)
- [x] 2026-04-17 - Fix: cursor lands on wrong item after move (cursor now tracks moved item)
- [x] 2026-04-17 - Fix: Enter dropped move; now Space picks and Space drops
- [x] 2026-04-17 - Fix: categories could be moved from right panel; now right panel moves feeds only
- [x] 2026-04-17 - Fix: fetching spinner shown after cache clear
- [x] 2026-04-17 - Fix: editor panel redesign — feeds left / categories right, Borders::ALL on each, feeds-only left
  panel, categories-only right panel, cursor highlight fixed, category move stays on categories panel
- [x] 2026-04-17 - Fix: auto-fetch disabled still showed spinner (feed.fetched now set true when auto_fetch_on_start is
  off)
