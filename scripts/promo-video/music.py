#!/usr/bin/env python3
"""Synthesizes the promo's soundtrack (numpy only, no samples, royalty free).

    node scripts/promo-video/render.mjs --cues target/promo/cues.json
    python scripts/promo-video/music.py target/promo/cues.json target/promo/music.wav

Music: 124 BPM electronic track in A minor (Am–F–C–G) that follows the video —
a build-up into a drop when the first feature appears, a fuller second groove,
a half-time section for "Safety first", a breakdown and second build into the
"And more" drop, and a big final hit for the outro. Kick-driven sidechain pumping
on the pads and bass.

Sound effects: one for every cue the page exports — pops for chips and cards
(rising in pitch when they come in a burst), ticks while numbers roll, dings when
they land, key clicks while the CLI types, a mouse click, whooshes between scenes,
impacts for titles, risers, scans and chimes.
"""

import argparse
import json
import wave

import numpy as np

SR = 44100
BPM = 124
BEAT = 60 / BPM
BAR = 4 * BEAT
S16 = BEAT / 4
rng = np.random.default_rng(2026)

# Am, F, C, G — pad voicings and bass roots
CHORDS = [[57, 60, 64, 69], [53, 57, 60, 65], [55, 60, 64, 67], [55, 59, 62, 67]]
ROOTS = [33, 29, 36, 31]
LEAD = [
    [76, None, 81, None, 84, 83, 81, None],
    [81, None, 84, None, 88, 86, 84, None],
    [79, None, 84, None, 88, None, 86, 84],
    [86, None, 83, None, 79, None, 81, None],
]
PENTA = [69, 72, 74, 76, 79, 81, 84, 86, 88, 91, 93, 96]


def hz(n):
    return 440.0 * 2 ** ((n - 69) / 12)


def tt(n):
    return np.arange(n) / SR


def ns(sec):
    return max(1, int(sec * SR))


# ── tiny DSP kit ──────────────────────────────────────────────────────────────
TL = 4096
_tables = {}


def table(kind, nh):
    key = (kind, nh)
    if key not in _tables:
        x = np.arange(TL) / TL
        w = np.zeros(TL)
        for k in range(1, nh + 1):
            if kind == "saw":
                w += np.sin(2 * np.pi * k * x) / k
            elif kind == "sq" and k % 2:
                w += np.sin(2 * np.pi * k * x) / k
        _tables[key] = w / np.abs(w).max()
    return _tables[key]


def osc(kind, f, n, nh=None, phase=None, vib=0.0):
    nh = nh or max(1, min(48, int(14000 / f)))
    ph = rng.random() if phase is None else phase
    fr = f * (1 + vib * np.sin(2 * np.pi * 5.5 * tt(n))) if vib else f
    p = (ph + np.cumsum(np.broadcast_to(fr, (n,))) / SR) % 1.0
    return table(kind, nh)[(p * TL).astype(np.int64)]


def supersaw(f, n, bright=1.0, voices=5, spread=14):
    """Detuned saws; `bright` (scalar or array 0..1) blends a dull and a full table."""
    out = np.zeros(n)
    for c in np.linspace(-spread, spread, voices):
        fv = f * 2 ** (c / 1200)
        ph = rng.random()
        dull = osc("saw", fv, n, nh=3, phase=ph)
        full = osc("saw", fv, n, phase=ph)
        out += dull + (full - dull) * bright
    return out / voices


def fir_lp(x, fc, taps=129):
    k = np.arange(taps) - (taps - 1) / 2
    h = np.sinc(2 * fc / SR * k) * np.hanning(taps)
    h /= h.sum()
    return np.convolve(x, h, mode="same")


def fir_hp(x, fc):
    return x - fir_lp(x, fc)


def bandpass(x, lo, hi):
    return fir_lp(x, hi) - fir_lp(x, lo)


def env(n, a=0.005, r=0.05):
    e = np.ones(n)
    na, nr = min(n, ns(a)), min(n, ns(r))
    e[:na] = np.linspace(0, 1, na)
    e[n - nr :] *= np.linspace(1, 0, nr)
    return e


def expdec(n, tau):
    return np.exp(-tt(n) / tau)


def fft_conv(x, h):
    n = len(x) + len(h) - 1
    m = 1 << (n - 1).bit_length()
    return np.fft.irfft(np.fft.rfft(x, m) * np.fft.rfft(h, m), m)[: len(x)]


class Bus:
    def __init__(self, total):
        self.b = np.zeros((ns(total + 3), 2))

    def add(self, t, sig, g=1.0, pan=0.0):
        i = int(round(t * SR))
        if i < 0:
            sig, i = sig[-i:], 0
        if i >= len(self.b) or len(sig) == 0:
            return
        sig = sig[: len(self.b) - i] * g
        a = (pan + 1) * np.pi / 4
        self.b[i : i + len(sig), 0] += sig * np.cos(a) * 1.414
        self.b[i : i + len(sig), 1] += sig * np.sin(a) * 1.414


# ── drums ─────────────────────────────────────────────────────────────────────
def kick():
    n = ns(0.45)
    t = tt(n)
    f = 46 + 130 * np.exp(-t / 0.035)
    body = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t / 0.22)
    click = rng.standard_normal(n) * np.exp(-t / 0.002) * 0.4
    return np.tanh(1.6 * (body + click))


def clap():
    n = ns(0.3)
    t = tt(n)
    noise = bandpass(rng.standard_normal(n), 900, 4200)
    e = np.zeros(n)
    for d in (0, 0.009, 0.018):
        m = t >= d
        e[m] += np.exp(-(t[m] - d) / 0.006)
    e += 0.5 * np.exp(-np.maximum(t - 0.022, 0) / 0.09) * (t > 0.022)
    return noise * e / np.abs(noise * e).max()


def snare():
    n = ns(0.22)
    t = tt(n)
    tone = np.sin(2 * np.pi * 190 * t) * np.exp(-t / 0.05)
    noise = fir_hp(rng.standard_normal(n), 1500) * np.exp(-t / 0.08)
    return (0.6 * tone + noise) / 1.5


def hat(open_=False):
    n = ns(0.25 if open_ else 0.05)
    s = fir_hp(rng.standard_normal(n), 7000) * expdec(n, 0.09 if open_ else 0.012)
    return s / np.abs(s).max()


def crash():
    n = ns(2.2)
    s = fir_hp(rng.standard_normal(n), 4500) * expdec(n, 0.7)
    return s / np.abs(s).max()


def riser(dur):
    n = ns(dur)
    t = tt(n)
    p = t / dur
    noise = rng.standard_normal(n)
    lo, hi = fir_lp(noise, 700), fir_hp(noise, 2500)
    sweep = np.sin(2 * np.pi * np.cumsum(200 * 12 ** p) / SR)
    s = ((1 - p) * lo * 2.5 + p * hi) * 0.7 + 0.25 * sweep
    return s * p**2.2


def sub_drop():
    n = ns(1.0)
    t = tt(n)
    f = 30 + 60 * np.exp(-t / 0.25)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * expdec(n, 0.4)


# ── sound effects ─────────────────────────────────────────────────────────────
def bell(f, dur=0.7, tau=0.25):
    n = ns(dur)
    t = tt(n)
    s = sum(a * np.sin(2 * np.pi * f * m * t) * np.exp(-t / (tau / (1 + i * 0.8)))
            for i, (m, a) in enumerate([(1, 1), (2.0, 0.5), (3.01, 0.25), (4.17, 0.15)]))
    return s * env(n, 0.002, 0.05)


def sfx_pop(f):
    n = ns(0.12)
    t = tt(n)
    fr = f * (1 + 0.5 * np.exp(-t / 0.012))
    s = np.sin(2 * np.pi * np.cumsum(fr) / SR) * np.exp(-t / 0.035)
    s += 0.3 * np.sin(2 * np.pi * np.cumsum(2 * fr) / SR) * np.exp(-t / 0.015)
    return s * env(n, 0.001, 0.02)


def sfx_swish(dur=0.22):
    n = ns(dur)
    p = np.linspace(0, 1, n)
    s = bandpass(rng.standard_normal(n), 1500, 6000)
    return s * np.sin(np.pi * p) ** 2 / 3


def sfx_whoosh(dur=0.85):
    n = ns(dur)
    p = np.linspace(0, 1, n)
    noise = rng.standard_normal(n)
    s = (1 - p) * fir_lp(noise, 1200) * 2 + p * bandpass(noise, 1500, 7000)
    return s * p**1.8 * (1 - p) ** 0.35 * 1.6


def sfx_impact():
    n = ns(0.9)
    t = tt(n)
    f = 38 + 70 * np.exp(-t / 0.06)
    boom = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t / 0.3)
    hit = fir_lp(rng.standard_normal(n), 3000) * np.exp(-t / 0.05)
    return np.tanh(1.5 * boom + 0.6 * hit)


def sfx_tick(v):
    n = ns(0.025)
    t = tt(n)
    f = 2600 * (1 + rng.uniform(-0.04, 0.04))
    s = np.sin(2 * np.pi * f * t) * np.exp(-t / 0.004) + 0.4 * rng.standard_normal(n) * np.exp(-t / 0.0015)
    return s * v


def sfx_click():
    a = np.zeros(ns(0.12))
    for d, g in ((0, 1.0), (0.075, 0.6)):
        n = ns(0.02)
        t = tt(n)
        c = bandpass(rng.standard_normal(n), 2500, 8000) * np.exp(-t / 0.002) + np.sin(2 * np.pi * 1700 * t) * np.exp(-t / 0.004)
        i = ns(d) if d else 0
        a[i : i + n] += c * g
    return a


def sfx_key():
    n = ns(0.035)
    t = tt(n)
    s = bandpass(rng.standard_normal(n), 1800, 6500) * np.exp(-t / 0.004) + 0.5 * np.sin(2 * np.pi * rng.uniform(380, 520) * t) * np.exp(-t / 0.008)
    return s * rng.uniform(0.6, 1.0)


def sfx_enter():
    n = ns(0.12)
    t = tt(n)
    return (bandpass(rng.standard_normal(n), 1200, 5000) * np.exp(-t / 0.006) + np.sin(2 * np.pi * 160 * t) * np.exp(-t / 0.03)) * 1.2


def glide(f0, f1, dur, kind="sin", trem=0.0):
    n = ns(dur)
    p = np.linspace(0, 1, n)
    f = f0 * (f1 / f0) ** p
    ph = np.cumsum(f) / SR
    s = np.sin(2 * np.pi * ph) if kind == "sin" else np.sign(np.sin(2 * np.pi * ph)) * 0.5 + 0.5 * np.sin(4 * np.pi * ph)
    if trem:
        s *= 0.6 + 0.4 * np.sin(2 * np.pi * trem * tt(n))
    return s * env(n, 0.03, 0.08)


def sfx_alarm():
    out = np.zeros(ns(0.45))
    for d in (0, 0.2):
        n = ns(0.13)
        b = (np.sign(np.sin(2 * np.pi * 880 * tt(n))) * 0.4 + np.sin(2 * np.pi * 1760 * tt(n)) * 0.3) * env(n, 0.003, 0.02)
        out[ns(d) : ns(d) + n] += b
    return out


# ── the track ─────────────────────────────────────────────────────────────────
def build(cues):
    sc = cues["scenes"]
    total = cues["total"]
    D1 = sc["problem"]["s"]
    SAFE = sc["safety"]["s"]
    LAST = sc["lastword"]["s"]
    MORE = sc["more"]["s"]
    OUT = sc["outro"]["s"]
    CLEAN = sc["clean"]["s"]

    drums, bass_b, pads, lead, fx_send = (Bus(total) for _ in range(5))
    kicks = []

    def K(t, g=1.0):
        drums.add(t, kick(), 0.95 * g)
        kicks.append(t)

    def chord_at(i):
        return CHORDS[i % 4], ROOTS[i % 4]

    def pad_bar(t, i, length, bright, g=0.11, top=True):
        ch, _ = chord_at(i)
        n = ns(length + 0.25)
        for j, m in enumerate(ch if top else ch[:3]):
            s = supersaw(hz(m), n, bright) * env(n, 0.02, 0.25)
            pads.add(t, s, g, pan=(j - 1.5) * 0.35)

    def bass_bar(t, i, style):
        _, r = chord_at(i)
        if style == "house":  # offbeat saw bass + sub on the one
            for b in range(4):
                n = ns(BEAT * 0.45)
                s = osc("saw", hz(r + 12), n, nh=10) * env(n, 0.004, 0.04)
                bass_b.add(t + b * BEAT + BEAT / 2, s, 0.22)
            n = ns(BEAT * 0.9)
            bass_b.add(t, np.sin(2 * np.pi * hz(r) * tt(n)) * env(n, 0.005, 0.08), 0.5)
        elif style == "roll":  # 16th-note rolling bass
            for k in range(16):
                if k % 4 == 0:
                    continue
                n = ns(S16 * 0.8)
                s = (osc("saw", hz(r + 12), n, nh=12) * 0.7 + np.sin(2 * np.pi * hz(r) * tt(n))) * env(n, 0.003, 0.02)
                bass_b.add(t + k * S16, s, 0.24)
            n = ns(BEAT * 0.9)
            bass_b.add(t, np.sin(2 * np.pi * hz(r) * tt(n)) * env(n, 0.005, 0.08), 0.5)
        else:  # long notes
            n = ns(BAR)
            s = (np.sin(2 * np.pi * hz(r) * tt(n)) + 0.3 * osc("saw", hz(r + 12), n, nh=6)) * env(n, 0.02, 0.3)
            bass_b.add(t, s, 0.36)

    def arp_bar(t, i, div, g):
        ch, _ = chord_at(i)
        pat = [ch[0] + 12, ch[1] + 12, ch[2] + 12, ch[3] + 12, ch[2] + 24, ch[3] + 12, ch[1] + 12, ch[2] + 12]
        steps = 16 if div == 16 else 8
        dt = BAR / steps
        for k in range(steps):
            n = ns(0.35)
            m = pat[k % 8]
            s = (osc("saw", hz(m), n, nh=6) * 0.6 + np.sin(2 * np.pi * hz(m) * tt(n))) * expdec(n, 0.09) * env(n, 0.002, 0.05)
            pads.add(t + k * dt, s, g * (1.0 if k % 2 == 0 else 0.75), pan=-0.4 if k % 2 else 0.4)
            fx_send.add(t + k * dt, s, g * 0.4)

    def lead_bar(t, i, g):
        line = LEAD[i % 4]
        k = 0
        while k < 8:
            m = line[k]
            if m is None:
                k += 1
                continue
            j = k + 1
            while j < 8 and line[j] is None:
                j += 1
            n = ns((j - k) * BEAT / 2 * 0.95)
            s = (supersaw(hz(m), n, 1.0, voices=3, spread=9) * 0.7 + osc("sq", hz(m), n, vib=0.004) * 0.3) * env(n, 0.006, 0.06)
            lead.add(t + k * BEAT / 2, s, g)
            fx_send.add(t + k * BEAT / 2, s, g * 0.6)
            k = j

    def drums_bar(t, style, last=False):
        if style in ("four", "four+"):
            for b in range(4):
                if not (last and b == 3):
                    K(t + b * BEAT)
            for b in (1, 3):
                drums.add(t + b * BEAT, clap(), 0.42)
            for k in range(8 if style == "four" else 16):
                dt = BAR / (8 if style == "four" else 16)
                off = (k % 2 == 1) if style == "four" else (k % 4 == 2)
                if style == "four+" and k % 4 == 2:
                    drums.add(t + k * dt, hat(True), 0.10, pan=0.25)
                else:
                    drums.add(t + k * dt, hat(), 0.13 if off else 0.07, pan=0.25)
        elif style == "half":
            K(t)
            K(t + 2.5 * BEAT, 0.6)
            drums.add(t + 2 * BEAT, clap(), 0.45)
            for k in range(8):
                drums.add(t + k * BEAT / 2, hat(), 0.06 if k % 2 else 0.04, pan=0.25)

    def roll(t_end, dur, g=0.35):
        """Snare roll accelerating into t_end."""
        t0 = t_end - dur
        divs = [(0.5, BEAT / 2), (0.75, BEAT / 4), (1.0, BEAT / 8)]
        t = t0
        while t < t_end - 1e-3:
            p = (t - t0) / dur
            step = next(d for lim, d in divs if p < lim)
            drums.add(t, snare(), g * (0.35 + 0.65 * p))
            t += step
        fx_send.add(t_end - dur, riser(dur), 0.18)
        drums.add(t_end - dur, riser(dur), 0.22)

    def drop(t):
        K(t, 1.1)
        drums.add(t, crash(), 0.28)
        drums.add(t, sub_drop(), 0.5)

    # 1 ─ intro build (0 → D1): pad opening up, kick creeping in, roll into the drop
    n_in = int(D1 / BAR) + 1
    for b in range(n_in):
        t = D1 - (n_in - b) * BAR
        i = b
        bright = np.clip((tt(ns(BAR + 0.25)) + t) / D1, 0, 1) ** 1.5 if t + BAR > 0 else 0.0
        pad_bar(t, i, BAR, bright, g=0.10)
        if t >= D1 - 2 * BAR:
            for k in range(4):
                K(t + k * BEAT, 0.55)
    roll(D1, BAR * 1.0)

    # 2 ─ grooves A and B (D1 → SAFE)
    nB = int((SAFE - D1) / BAR)
    gB = round((CLEAN - D1) / BAR)
    for b in range(nB):
        t = D1 + b * BAR
        last = b == nB - 1
        second = b >= gB
        drums_bar(t, "four+" if second else "four", last=last)
        bass_bar(t, b, "roll" if second else "house")
        pad_bar(t, b, BAR, 0.85 if second else 0.6, g=0.09)
        arp_bar(t, b, 16 if second else 8, 0.07)
        if second:
            lead_bar(t, b, 0.075)
        if b % 8 == 0 and b:
            drums.add(t, crash(), 0.2)
        if b == gB:
            drop(t)
    roll(SAFE, BAR * 0.75, 0.25)

    # 3 ─ half-time for Safety, breakdown for "You decide", build into MORE
    nC = int((MORE - SAFE) / BAR) + 1
    brk = SAFE + round((LAST - SAFE) / BAR) * BAR
    drop(SAFE)
    for b in range(nC):
        t = SAFE + b * BAR
        if t >= MORE - 0.05:
            break
        length = min(BAR, MORE - t)
        if t < brk:
            drums_bar(t, "half")
            bass_bar(t, b, "long")
            pad_bar(t, b, length, 0.9, g=0.1)
            arp_bar(t, b, 8, 0.06)
        else:
            pad_bar(t, b, length, 0.35 + 0.65 * (t - brk) / (MORE - brk), g=0.11)
            arp_bar(t, b, 16, 0.045)
    roll(MORE, BAR * 2, 0.32)

    # 4 ─ final drop (MORE → OUT)
    drop(MORE)
    nD = int((OUT - MORE) / BAR)
    for b in range(nD):
        t = MORE + b * BAR
        drums_bar(t, "four+", last=b == nD - 1)
        bass_bar(t, b, "roll")
        pad_bar(t, b, BAR, 1.0, g=0.09)
        arp_bar(t, b, 16, 0.065)
        lead_bar(t, b, 0.085)
        if b % 4 == 0 and b:
            drums.add(t, crash(), 0.18)
    tail = MORE + nD * BAR
    if OUT - tail > 0.15:
        fx_send.add(tail, riser(OUT - tail), 0.25)

    # 5 ─ outro: one big hit and a ringing chord
    drop(OUT)
    n = ns(total - OUT + 1)
    for j, m in enumerate([57, 64, 69, 72, 76]):
        s = supersaw(hz(m), n, np.clip(1 - tt(n) / 7, 0.2, 1)) * env(n, 0.01, 2.5)
        pads.add(OUT, s, 0.085, pan=(j - 2) * 0.3)
        fx_send.add(OUT, s, 0.05)
    bass_b.add(OUT, np.sin(2 * np.pi * hz(33) * tt(n)) * expdec(n, 1.5), 0.45)
    for k, m in enumerate([81, 84, 88, 93, 88, 84, 81, 76]):
        s = bell(hz(m), 1.2, 0.35)
        pads.add(OUT + 0.9 + k * BEAT, s, 0.05 * (1 - k / 10), pan=-0.3 if k % 2 else 0.3)
        fx_send.add(OUT + 0.9 + k * BEAT, s, 0.05)

    # sidechain pump on pads and bass from every kick
    m = len(pads.b)
    tl = np.arange(m) / SR
    kt = np.array(sorted(kicks))
    idx = np.searchsorted(kt, tl, side="right") - 1
    dt = np.where(idx >= 0, tl - kt[np.clip(idx, 0, None)], 10.0)
    pump = 1 - 0.6 * np.exp(-dt / 0.13) * (1 - np.exp(-dt / 0.004))
    music = drums.b + (bass_b.b + pads.b) * pump[:, None] + lead.b
    return music, fx_send, total


def sfx(cues, total):
    bus, send = Bus(total), Bus(total)
    seq, last_pop, last_swish = 0, -9.0, -9.0
    paint_k = 0
    for c in cues["cues"]:
        t, ty, v = c["t"], c["type"], c.get("v", 1)
        if ty == "pop":
            seq = seq + 1 if t - last_pop < 0.3 else rng.integers(0, 3)
            last_pop = t
            s = sfx_pop(hz(PENTA[min(seq, len(PENTA) - 1)]))
            bus.add(t, s, 0.16, pan=rng.uniform(-0.4, 0.4))
            send.add(t, s, 0.08)
        elif ty in ("swish", "move"):
            if t - last_swish > 0.18:
                bus.add(t - 0.05, sfx_swish(), 0.10, pan=rng.uniform(-0.5, 0.5))
                last_swish = t
        elif ty == "whoosh":
            bus.add(t - 0.6, sfx_whoosh(), 0.22)
        elif ty == "impact":
            bus.add(t, sfx_impact(), 0.35)
        elif ty == "draw":
            bus.add(t, glide(500, 1400, 0.5), 0.05)
        elif ty == "paint":
            s = bell(hz(PENTA[paint_k % len(PENTA)]), 0.4, 0.08)
            paint_k += 1
            bus.add(t, s, 0.07, pan=-0.6 + 1.2 * paint_k / 15)
            send.add(t, s, 0.05)
        elif ty == "tick":
            bus.add(t, sfx_tick(v), 0.13, pan=rng.uniform(-0.15, 0.15))
        elif ty == "ding":
            s = bell(hz(91), 0.9, 0.22)
            bus.add(t, s, 0.13 * v)
            send.add(t, s, 0.1)
        elif ty == "success":
            for d, m in ((0, 88), (0.09, 93), (0.18, 100)):
                s = bell(hz(m), 1.0, 0.25)
                bus.add(t + d, s, 0.12)
                send.add(t + d, s, 0.1)
        elif ty == "alarm":
            bus.add(t, sfx_alarm(), 0.13)
        elif ty == "rise":
            bus.add(t, glide(180, 900, c["dur"], trem=9), 0.06)
        elif ty == "scan":
            s = glide(500, 2200, c["dur"], trem=18)
            bus.add(t, s, 0.05)
            bus.add(t, sfx_swish(c["dur"]), 0.08)
        elif ty == "zap":
            bus.add(t, glide(250, 1600, c["dur"], kind="sq"), 0.035)
            bus.add(t + c["dur"], bell(hz(88), 0.8, 0.2), 0.1)
        elif ty == "click":
            bus.add(t, sfx_click(), 0.45)
        elif ty == "key":
            bus.add(t, sfx_key(), 0.16, pan=rng.uniform(-0.2, 0.2))
        elif ty == "enter":
            bus.add(t, sfx_enter(), 0.25)
        elif ty == "line":
            bus.add(t, sfx_pop(hz(84)), 0.05 * v)
    return bus, send


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cues")
    ap.add_argument("out")
    a = ap.parse_args()
    cues = json.load(open(a.cues, encoding="utf-8"))
    music, send, total = build(cues)
    fx, fx_send = sfx(cues, total)

    # one shared reverb for the send buses
    n = ns(2.2)
    ir = np.stack([fir_lp(rng.standard_normal(n), 6000) * expdec(n, 0.5) for _ in range(2)], 1)
    ir /= np.sqrt((ir**2).sum(0))
    wet_in = send.b + fx_send.b
    wet = np.stack([fft_conv(wet_in[:, c], ir[:, c]) for c in range(2)], 1)

    mix = music * 0.8 + wet * 0.9 + fx.b
    mix = mix[: ns(total)]
    tl = tt(len(mix))
    mix *= (np.clip(tl / 0.3, 0, 1) * np.clip((total - tl) / 2.0, 0, 1))[:, None]
    mix /= np.percentile(np.abs(mix), 99.9) / 0.8
    mix = np.tanh(mix * 1.1) / np.tanh(1.1)
    mix *= 0.97 / np.abs(mix).max()
    pcm = (mix * 32767).astype(np.int16)
    with wave.open(a.out, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    rms = 20 * np.log10(np.sqrt((mix**2).mean()))
    print(f"{a.out}  {total:.2f} s  RMS {rms:.1f} dBFS")


if __name__ == "__main__":
    main()
