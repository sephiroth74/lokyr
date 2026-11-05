# The window's motion, its resize and the cursor's fades, against Ghostty's (F3-152)

Measured on 2026-11-05, between 09:07 and 09:44, of the session's script, its analysis and their figures in [motion-F3-152](motion-F3-152/), `run.sh`, `analyze.py`, `figures.csv` and the probe's reports: whether a window's drag, its resize, the translucency, the blur and the cursor's fades cost Lokyr frames that Ghostty does not lose, as the user found a drag less fluid than Ghostty's and a powerline prompt that flickered through a resize.

## Method

- **The probe**: `lokyr-bench motion` drags a window by its titlebar, or resizes it by its bottom-right corner, along a sine of 200 points and of 2 s, for 10 s, of the mouse's synthetic events at 120 Hz, as a hand would; Instruments' Animation Hitches records every process meanwhile, `xctrace record --all-processes`, and `tools/bench/motion_extract.py` reads the recording, of the motion less its first and its last 0.3 s: the frames that the display showed, the window server's surfaces' swaps, a second; those that it missed, as the gaps between two swaps longer than a refresh and a half count them; the compositor's delay of each, its median and its 95th percentile; and the window server's and the app's CPU, in percent of a core.
- **Nine configurations**, each of Menlo 20 and 88 by 27 cells: Lokyr opaque, translucent, `background-opacity = 0.9`, and translucent with a blur of 10, each with the cursor's fades of the user's config, 100, 500, 500 and 100 ms, and without them, a switch of the same cycle, 600 and 600 ms; Ghostty 1.x in the same three, of the options of its command line alone, `--config-default-files=false`, its cursor a switch, as it has no fades. The display, the Mac's built-in one, ran at 60 Hz.
- **Two rounds**, the second in the reverse order.

## Results

The means of the two rounds; 9.4 s of motion a run.

### A drag

| Configuration | Frames a second | Missed | Delay p50 | p95 | Window server's CPU | App's CPU |
|---|---|---|---|---|---|---|
| Lokyr, opaque, a switch | 57.5 | 20.5 | 13.4 ms | 27.2 ms | 24 % | 3 % |
| Lokyr, opaque, fades | 58.6 | 10.0 | 13.1 ms | 20.6 ms | 32 % | 3 % |
| Lokyr, translucent, a switch | 57.6 | 19.0 | 13.1 ms | 14.0 ms | 31 % | 4 % |
| Lokyr, translucent, fades | 57.2 | 23.0 | 13.3 ms | 21.0 ms | 28 % | 3 % |
| Lokyr, blur, a switch | 58.2 | 13.5 | 13.1 ms | 21.8 ms | 29 % | 3 % |
| Lokyr, blur, fades | 57.9 | 17.5 | 12.8 ms | 27.1 ms | 34 % | 3 % |
| Ghostty, opaque | 58.3 | 13.0 | 12.5 ms | 13.5 ms | 36 % | 4 % |
| Ghostty, translucent | 57.9 | 16.5 | 12.7 ms | 21.5 ms | 33 % | 5 % |
| Ghostty, blur | 57.3 | 21.0 | 12.4 ms | 21.0 ms | 35 % | 5 % |

### A resize

| Configuration | Frames a second | Missed | Delay p50 | p95 | Window server's CPU | App's CPU |
|---|---|---|---|---|---|---|
| Lokyr, opaque, a switch | 56.6 | 28.5 | 12.0 ms | 19.4 ms | 39 % | 17 % |
| Lokyr, opaque, fades | 56.8 | 27.0 | 11.9 ms | 13.3 ms | 40 % | 17 % |
| Lokyr, translucent, a switch | 56.6 | 28.5 | 12.1 ms | 27.8 ms | 40 % | 16 % |
| Lokyr, translucent, fades | 56.6 | 28.0 | 12.0 ms | 20.5 ms | 39 % | 16 % |
| Lokyr, blur, a switch | 56.8 | 26.0 | 12.1 ms | 28.2 ms | 37 % | 15 % |
| Lokyr, blur, fades | 56.8 | 27.0 | 11.8 ms | 20.4 ms | 40 % | 16 % |
| Ghostty, opaque | 57.0 | 25.0 | 11.3 ms | 27.7 ms | 43 % | 35 % |
| Ghostty, translucent | 56.6 | 28.5 | 11.9 ms | 20.3 ms | 40 % | 32 % |
| Ghostty, blur | 57.7 | 17.5 | 11.2 ms | 27.5 ms | 44 % | 34 % |

- **A drag shows as many frames in Lokyr as in Ghostty**, 57 to 59 a second of 60, in every configuration: the differences are those between two rounds of one configuration. Lokyr draws nothing as its window moves; the window server moves it.
- **The fades and the translucency cost the window server**, not frames: of a drag, 24 % of a core for Lokyr opaque of a switch, 28 to 34 % for the others, Ghostty 33 to 36 %.
- **A resize draws at half Ghostty's rate**: the frames that the display shows are alike, the window server's, but Lokyr's own come every 33 ms, 29.8 a second in a run of its Metal command buffers, and Ghostty's at the display's rate, 87 of its command buffers a second, at twice Lokyr's CPU. Three drawables, the build's feature `three-drawables`, draw 29.3 a second: the drawables are not what halves it. A step of the resize that comes within a refresh of the last frame waits for the display link's next refresh, as F3-141 made it to keep a frame from waiting for a drawable within Core Animation's flush, and its frame then comes of a transaction of its own.

## The prompt's flicker

A slow-motion recording of the user's, 240 frames a second, showed the powerline prompt's line gone for about 190 ms at a time through a resize in Lokyr, and never in Ghostty. zsh draws the prompt again in one write within a millisecond of each signal, 40 of 40 sizes told in a second. A log of the pane's steps through a resize of the user's shell found the cause: the screen waited for output from the first size that the program had not answered, so that the program's answer to one size, which no frame had copied yet, ended the wait of the next, whose step had cleared the prompt; 30 of 71 copies of the screen showed it cleared. Each size now waits for output after its own signal, 1e55431; of the next resize, no copy showed it cleared, 45 of 45, and the user found the flicker gone.
