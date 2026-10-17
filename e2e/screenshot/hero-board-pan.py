#!/usr/bin/env python3
"""Pan the board inside an already-recorded hero segment.

The page stays scrolled to the left edge. This replaces the board rectangle
with a crop of the full column track so a later column can slide into view
without a column resting half off the frame. The caption pill is copied back
from the original frame and stays put. A frame whose pill was hidden for the
track shot reuses the last frame that still had it, so the caption does not
blink or travel with the pan.
"""

import argparse
import subprocess
import sys

from PIL import Image, ImageDraw


def ease(progress):
    from math import cos, pi

    progress = max(0.0, min(1.0, progress))
    return 0.5 - 0.5 * cos(pi * progress)


def stadium_mask(width, height):
    mask = Image.new("L", (width, height), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, width - 1, height - 1),
        radius=height / 2,
        fill=255,
    )
    return mask


def paste_track(frame, original, track, board, pad, scroll):
    bx, by, bw, bh = board
    background = original.getpixel((min(bx + 8, frame.width - 1), min(by + 8, frame.height - 1)))
    ImageDraw.Draw(frame).rectangle((bx, by, bx + bw - 1, by + bh - 1), fill=background)
    dest_x = int(round(bx + pad - scroll))
    dest_y = by
    src_x = 0
    if dest_x < bx:
        src_x += bx - dest_x
        dest_x = bx
    max_w = bx + bw - dest_x
    src_w = min(track.width - src_x, max_w)
    src_h = min(track.height, bh)
    if src_w <= 0 or src_h <= 0:
        return
    crop = track.crop((src_x, 0, src_x + src_w, src_h))
    frame.paste(crop, (dest_x, dest_y))


def pill_present(piece):
    """The crop holds the dark caption pill, not the light board behind it."""
    width, height = piece.size
    x0, y0 = int(width * 0.28), int(height * 0.28)
    x1, y1 = int(width * 0.72), int(height * 0.72)
    if x1 <= x0 or y1 <= y0:
        return False
    dark = 0
    total = 0
    for red, green, blue in piece.crop((x0, y0, x1, y1)).getdata():
        total += 1
        if red < 110 and green < 105 and blue < 100:
            dark += 1
    return total > 0 and dark >= max(8, total // 25)


def restore_caption(frame, original, caption, held):
    if caption is None:
        return
    x, y, w, h = caption
    pad = 10
    x = max(0, int(x) - pad)
    y = max(0, int(y) - pad)
    w = int(w) + pad * 2
    h = int(h) + pad * 2
    w = min(w, frame.width - x)
    h = min(h, frame.height - y)
    if w < 8 or h < 8:
        return
    piece = original.crop((x, y, x + w, y + h))
    if pill_present(piece):
        held["piece"] = piece
    elif held.get("piece") is not None and held["piece"].size == piece.size:
        piece = held["piece"]
    else:
        return
    frame.paste(piece, (x, y), stadium_mask(w, h))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--video", required=True)
    parser.add_argument("--track", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--start", type=float, required=True)
    parser.add_argument("--duration", type=float, required=True)
    parser.add_argument("--scroll", type=float, required=True)
    parser.add_argument("--board", required=True, help="x,y,w,h")
    parser.add_argument("--pad", type=float, required=True)
    parser.add_argument("--caption", default="", help="x,y,w,h or empty")
    parser.add_argument("--fps", type=float, default=25)
    args = parser.parse_args()

    board = tuple(int(round(float(part))) for part in args.board.split(","))
    caption = None
    if args.caption:
        caption = tuple(float(part) for part in args.caption.split(","))
    track = Image.open(args.track).convert("RGB")

    decode = subprocess.Popen(
        [
            "ffmpeg",
            "-v",
            "error",
            "-i",
            args.video,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ],
        stdout=subprocess.PIPE,
    )
    encode = subprocess.Popen(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            "1440x900",
            "-r",
            str(args.fps),
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "16",
            "-preset",
            "veryfast",
            args.out,
        ],
        stdin=subprocess.PIPE,
    )
    frame_bytes = 1440 * 900 * 3
    index = 0
    held = {}
    try:
        while True:
            blob = decode.stdout.read(frame_bytes)
            if len(blob) < frame_bytes:
                break
            original = Image.frombytes("RGB", (1440, 900), blob)
            time = index / args.fps
            if time + 1e-6 < args.start:
                frame = original
            else:
                progress = (time - args.start) / max(args.duration, 0.01)
                scroll = args.scroll * ease(progress)
                frame = original.copy()
                paste_track(frame, original, track, board, args.pad, scroll)
                restore_caption(frame, original, caption, held)
            encode.stdin.write(frame.tobytes())
            index += 1
    finally:
        decode.stdout.close()
        decode.wait()
        encode.stdin.close()
        code = encode.wait()
    if code != 0 or index == 0:
        sys.exit(f"board pan failed ({code}) after {index} frames")


if __name__ == "__main__":
    main()
