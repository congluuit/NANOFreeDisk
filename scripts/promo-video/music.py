#!/usr/bin/env python3
"""Synthesizes the promo's background music (numpy only, no samples, royalty free).

    python scripts/promo-video/music.py <out.wav>

Soft pad + plucked arpeggio + bass and a light beat in A minor (Am–F–C–G, 96 BPM),
a whoosh before every scene change, a calm intro and outro. Scene times follow the
data-dur values of index.html (keep DURS in sync).
"""

import argparse
import wave

import numpy as np

SR = 44100
BPM = 96
BEAT = 60 / BPM
DURS = [7.5, 8.5, 9.5, 11, 10.5, 10.5, 11, 10.5, 10.5, 10.5, 10.5, 12, 9, 11, 10, 9]
OV = 0.6


def midi(n):
    return 440.0 * 2 ** ((n - 69) / 12)


# Am, F, C, G — two bars each (8 beats)
CHORDS = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]]


def env_adsr(n, a, r):
    e = np.ones(n)
    na, nr = int(a * SR), int(r * SR)
    e[:na] = np.linspace(0, 1, na) if na else 1
    if nr:
        e[-nr:] *= np.linspace(1, 0, nr)
    return e


def add(buf, start, sig, pan=0.0):
    i = int(start * SR)
    if i >= buf.shape[0]:
        return
    sig = sig[: buf.shape[0] - i]
    l, r = np.cos((pan + 1) * np.pi / 4), np.sin((pan + 1) * np.pi / 4)
    buf[i : i + len(sig), 0] += sig * l
    buf[i : i + len(sig), 1] += sig * r


def pad_note(f, dur):
    t = np.arange(int(dur * SR)) / SR
    s = (np.sin(2 * np.pi * f * t) + 0.5 * np.sin(2 * np.pi * f * 1.004 * t + 1) + 0.5 * np.sin(2 * np.pi * f * 0.996 * t + 2)
         + 0.15 * np.sin(2 * np.pi * 2 * f * t) + 0.06 * np.sin(2 * np.pi * 3 * f * t))
    s *= 1 + 0.15 * np.sin(2 * np.pi * 0.3 * t)  # slow swell
    return s * env_adsr(len(t), 1.4, 1.6) / 2.2


def pluck(f, dur=0.9):
    t = np.arange(int(dur * SR)) / SR
    s = np.sin(2 * np.pi * f * t) + 0.35 * np.sin(2 * np.pi * 2 * f * t) * np.exp(-t * 9) + 0.1 * np.sin(2 * np.pi * 3 * f * t) * np.exp(-t * 14)
    return s * np.exp(-t * 5.5) * env_adsr(len(t), 0.004, 0.05)


def bass(f, dur):
    t = np.arange(int(dur * SR)) / SR
    s = np.sin(2 * np.pi * f * t) + 0.25 * np.sin(2 * np.pi * 2 * f * t)
    return s * np.exp(-t * 1.6) * env_adsr(len(t), 0.01, 0.15)


def kick():
    t = np.arange(int(0.35 * SR)) / SR
    f = 50 + 70 * np.exp(-t * 30)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 11)


rng = np.random.default_rng(7)


def hat():
    n = int(0.08 * SR)
    s = np.diff(rng.standard_normal(n + 1))  # crude high-pass
    return s * np.exp(-np.arange(n) / SR * 60) * 0.5


def whoosh(dur=0.9):
    n = int(dur * SR)
    noise = rng.standard_normal(n)
    # band-pass via FFT on the whole burst with a rising-then-falling envelope
    spec = np.fft.rfft(noise)
    freqs = np.fft.rfftfreq(n, 1 / SR)
    spec *= np.exp(-((np.log(freqs + 1) - np.log(1800)) ** 2) / 1.2)
    s = np.fft.irfft(spec, n)
    s /= np.abs(s).max() + 1e-9
    t = np.linspace(0, 1, n)
    return s * (t ** 2.2) * (1 - t) ** 0.4 * 2.2


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    a = ap.parse_args()
    starts, acc = [], 0.0
    for d in DURS:
        starts.append(acc)
        acc += d - OV
    total = starts[-1] + DURS[-1]
    outro = starts[-1]
    buf = np.zeros((int((total + 2) * SR), 2))

    bar2 = 8 * BEAT
    k = 0
    t = 0.0
    while t < total:
        ch = CHORDS[k % 4]
        for j, n in enumerate(ch):
            add(buf, t, pad_note(midi(n), bar2 + 1.6) * 0.16, pan=(j - 1) * 0.5)
        add(buf, t, pad_note(midi(ch[0] - 12), bar2 + 1.6) * 0.10)
        groove = 4.2 <= t < outro + 1.0
        if groove:
            arp = [ch[0] + 12, ch[1] + 12, ch[2] + 12, ch[1] + 24, ch[0] + 24, ch[2] + 12, ch[1] + 12, ch[2] + 24]
            for i in range(16):
                tt = t + i * BEAT / 2
                add(buf, tt, pluck(midi(arp[i % 8])) * (0.085 if i % 2 == 0 else 0.06), pan=0.35 if i % 2 else -0.35)
            for i in range(8):
                tt = t + i * BEAT
                if i % 4 in (0, 3):
                    add(buf, tt, bass(midi(ch[0] - 24), BEAT * 1.5) * 0.22)
                if i % 2 == 0:
                    add(buf, tt, kick() * 0.30)
                add(buf, tt + BEAT / 2, hat() * 0.05, pan=0.2)
        k += 1
        t += bar2

    for s in starts[1:]:
        add(buf, s - 0.75, whoosh() * 0.05)

    # final chord ring-out for the outro
    for j, n in enumerate([57, 60, 64, 69]):
        add(buf, outro + 0.4, pad_note(midi(n), 8.0) * 0.12, pan=(j - 1.5) * 0.3)

    buf = buf[: int(total * SR)]
    tt = np.arange(len(buf)) / SR
    fade = np.clip(tt / 2.0, 0, 1) * np.clip((total - tt) / 3.5, 0, 1)
    buf *= fade[:, None]
    buf /= np.abs(buf).max() / 0.7
    pcm = (buf * 32767).astype(np.int16)
    with wave.open(a.out, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    print(a.out, f"{total:.2f} s")


if __name__ == "__main__":
    main()
