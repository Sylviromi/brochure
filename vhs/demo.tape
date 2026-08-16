# brochure demo tape
# Requires: cargo build --release (binary at target/release/brochure)
#
# Usage (from repo root):
#   vhs vhs/demo.tape
#   vhs publish vhs/demo.gif
#
# Notes:
# - Generous sleeps after every key: VHS occasionally drops a keypress
#   (go-rod -> xterm.js -> ttyd), and a dropped key derails the whole demo.
#   The tape is deliberately lean (~25 keys); the article section is tolerant
#   of dropped navigation keys (it still opens a feed and an article).
# - After generating, verify the demo recorded correctly before publishing:
#   python3 scripts/verify_demo.py  (regenerates until verified)

Output vhs/demo.gif

# Dark terminal background keeps the app's colours readable (recipe TL;DR).
Set Theme "Aardvark Blue"
Set FontSize 14
Set Width 1200
Set Height 900
Set Padding 0
Set Framerate 24

Hide
Type "./target/release/brochure"
Enter
Sleep 1s
Show
Sleep 3s # skeleton animation + feeds loading

# ---- Tab tour: Feeds -> Saved -> Settings -> Changelog -> Feeds ----
Tab
Sleep 1s
Tab
Sleep 1s
Tab
Sleep 1s
Sleep 2.5s # let the viewer read the About/Changelog screen
Tab
Sleep 2s # back on Feeds

# ---- Open Scientific American and its first article ----
Down
Sleep 900ms
Down
Sleep 900ms
Enter
Sleep 2s
Enter
Sleep 3s

# ---- Scroll the article to show the inline image ----
PageDown
Sleep 1.5s
PageDown
Sleep 1.5s
PageDown
Sleep 2s

# ---- Zen mode: distraction-free reading ----
Type "z"
Sleep 2.5s
PageDown
Sleep 2.5s
Escape
Sleep 1s

# ---- Back to the article list; save the article ----
Escape
Sleep 900ms
Type "s" # category picker
Sleep 1.5s
Down
Sleep 900ms
Enter # save
Sleep 1s
Escape
Sleep 900ms

# ---- Saved tab: browse the saved article ----
Tab
Sleep 1.5s
Down # "All Saved" -> "Read Later"
Sleep 900ms
Enter
Sleep 2s
Enter # open the saved article
Sleep 2.5s
Escape
Sleep 900ms
Escape
Sleep 1s

# ---- Settings tab ----
Tab
Sleep 1.5s
Sleep 2.5s # show the Settings screen

Sleep 2s
