# Black & Pink Crosshair

The Black & Pink Crosshair cursor theme uses editable shapes in `artwork.svg`. `build.py` renders those shapes into the 40 px cursor files under `cursors/`. The dark outline follows the pink strokes on both sides instead of sitting below and to the right.

After editing the SVG, rebuild the cursor files with:

    python3 build.py

The build needs Python 3 and `resvg`. CI pins resvg 0.48.1, so rebuild the stored cursors with that version to keep them identical to the test build. To check the output without replacing the stored cursor files, run `python3 build.py --output-dir /path/to/empty/directory`. The animated cursors keep their frame counts and timing in `build.py`; cursor names and aliases remain under `cursors/`.

The theme takes its crosshair idea and pink palette from the Crosshair Cursors set by AarogyaGaming. The original set is public domain: <http://www.rw-designer.com/cursor-set/crosshar>.
