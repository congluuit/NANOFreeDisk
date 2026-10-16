#!/usr/bin/env python3
"""Turn screencast frames (from scripts/demo-video.mjs) into animations.

    python scripts/frames-to-video.py <frames-dir> <output-base> [--fps 8] [--width 960]

Reads <frames-dir>/frames.json ([{file, t}] with t in seconds), resamples the frames
to a constant frame rate (each output frame shows the latest captured frame) and
writes <output-base>.gif and <output-base>.webp with Pillow. When ffmpeg is on PATH it
also writes <output-base>.mp4 (H.264, from the full-size frames).
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image


def resample(shots, fps):
    """Indices into `shots` for a constant-rate timeline."""
    if not shots:
        return []
    start, end = shots[0]["t"], shots[-1]["t"] + 1.0
    out, j = [], 0
    t = start
    while t <= end:
        while j + 1 < len(shots) and shots[j + 1]["t"] <= t:
            j += 1
        out.append(j)
        t += 1.0 / fps
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("frames")
    ap.add_argument("out")
    ap.add_argument("--fps", type=int, default=8)
    ap.add_argument("--width", type=int, default=960)
    a = ap.parse_args()
    frames = Path(a.frames)
    shots = json.loads((frames / "frames.json").read_text(encoding="utf-8"))
    order = resample(shots, a.fps)
    if not order:
        sys.exit("no frames")

    # every frame gets the size most frames have (the first ones can be captured
    # before the viewport size is fixed)
    from collections import Counter

    sizes = Counter(Image.open(frames / s["file"]).size for s in shots)
    (w0, h0), _ = sizes.most_common(1)[0]
    size = (a.width, round(h0 * a.width / w0))
    cache = {}

    def load(i):
        if i not in cache:
            im = Image.open(frames / shots[i]["file"]).convert("RGB")
            cache[i] = im.resize(size, Image.LANCZOS)
        return cache[i]

    images = [load(i) for i in order]
    duration = round(1000 / a.fps)
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)

    # one shared palette keeps the GIF small and flicker-free
    palette_src = images[len(images) // 2].quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    gif = [im.quantize(palette=palette_src, dither=Image.Dither.NONE) for im in images]
    gif[0].save(out.with_suffix(".gif"), save_all=True, append_images=gif[1:], duration=duration, loop=0, optimize=True, disposal=1)
    images[0].save(out.with_suffix(".webp"), save_all=True, append_images=images[1:], duration=duration, loop=0, quality=72, method=4)

    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg:
        with tempfile.TemporaryDirectory() as tmp:
            for n, i in enumerate(order):
                shutil.copy(frames / shots[i]["file"], Path(tmp) / f"{n:05d}.jpg")
            subprocess.run(
                [ffmpeg, "-y", "-loglevel", "error", "-framerate", str(a.fps), "-i", str(Path(tmp) / "%05d.jpg"),
                 "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(out.with_suffix(".mp4"))],
                check=True,
            )
    for ext in (".gif", ".webp", ".mp4"):
        p = out.with_suffix(ext)
        if p.exists():
            print(f"{p}: {p.stat().st_size / 1_048_576:.1f} MiB, {len(order)} frames at {a.fps} fps")
    if not ffmpeg:
        print("ffmpeg not found: MP4 skipped (GIF and WebP written)")


if __name__ == "__main__":
    main()
