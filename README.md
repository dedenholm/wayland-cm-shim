# cm-shim

**The problem.** Wayland compositors assume every window is sRGB. But apps like
darktable do their own color management and hand over Adobe RGB pixels. The
compositor then converts them a second time, as if they were sRGB, and everything
comes out oversaturated. There's no setting anywhere to fix it: the app would have
to speak the Wayland color management protocol, and almost none of them do.

**The solution.** cm-shim sits between the app and the compositor and tells the
compositor which space the app is actually using. The app doesn't need to know it
exists. No patches, no forks, no recompiling anything.

```sh
cm-shim -s adobe_rgb run darktable
```

---

# 1. Install and use

## Dependencies

You need Rust to build it, `notify-send` so the shim can warn you when color
management fails, and `wayland-info` to check what your compositor supports.

**Arch**

```sh
sudo pacman -S --needed rust gcc libnotify wayland-utils git
```

**Debian / Ubuntu**

```sh
sudo apt install cargo build-essential libnotify-bin wayland-utils git
```

**Fedora**

```sh
sudo dnf install cargo gcc libnotify wayland-utils git
```

Those packages include both `rustc` and `cargo`, so you don't need rustup for
this. Check what you've got:

```sh
rustc --version
```

cm-shim needs **1.70 or newer**. Arch, Fedora, Debian 13 and Ubuntu 24.04 are all
comfortably past that. Two cases aren't:

- **Debian 12 (bookworm)** ships 1.63.
- **Ubuntu 22.04** ships 1.58 out of the box. The 1.75 backport in
  `jammy-updates` is fine, so `sudo apt update && sudo apt upgrade` may be all you
  need.

If you're stuck below 1.70, get Rust from rustup instead of your package manager:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

(If you installed rustup from your distro's repo rather than that script, also run
`rustup default stable` — the packaged rustup arrives with no toolchain at all.)

## Build

```sh
git clone <this repository>
cd cm-shim
./install.sh
```

The binary goes to `~/.local/bin/cm-shim`. If that isn't on your `PATH`, the
script offers `/usr/local/bin` instead and asks for `sudo` only for that one copy.

It also writes a fully commented config to `~/.config/cm-shim/config`. Everything
in it is commented out, so it changes nothing until you edit it.

To remove the binary and every launcher cm-shim made:

```sh
./install.sh uninstall
```

Your config file is left alone.

## Check your compositor

```sh
wayland-info | grep -i color
```

You need `wp_color_manager_v1` in the output. Known to work: **KWin** (Plasma 6)
and **Hyprland**. On KDE, color management also has to be switched on in Display
settings.

## Set it up

Two settings have to agree with each other:

| In your app | In cm-shim |
|---|---|
| Monitor profile → Adobe RGB | `-s adobe_rgb` |

The app renders as if your monitor were an Adobe RGB display. The shim tells the
compositor the window is Adobe RGB. The compositor converts from there to your
actual monitor.

If the two disagree, your colors are wrong and nothing will warn you about it.

## Run

```sh
cm-shim -s adobe_rgb run darktable
cm-shim -s rec2020_g22 -i relative run krita
```

Only the app you launch this way is affected. Everything else on your desktop is
untouched.

If you name no space anywhere — no flag, no config file — the shim declares
`adobe_rgb` and tells you it's doing so:

```
[cm-shim] no space given on the command line or in ~/.config/cm-shim/config;
[cm-shim] using the default: adobe_rgb.
[cm-shim] set your app's display profile to Adobe RGB (1998) to match,
[cm-shim] or choose another space with -s (see --help).
```

That default only covers half the setup. The app's own display profile still has
to be Adobe RGB, and nothing but you can set that.

## Make it permanent

```sh
cm-shim -s adobe_rgb install darktable
cm-shim uninstall darktable
```

`install` copies the app's launcher into `~/.local/share/applications/`, with
every `Exec=` line routed through the shim, and whatever flags you passed baked
in. It also sets `DBusActivatable=false`, since D-Bus activation would start the
app without ever reading `Exec`.

`<app>` can be the desktop file name (`org.gimp.GIMP`), part of it, or the program
name. If several match, the shim lists them and changes nothing.

Only launchers carrying cm-shim's marker line are ever overwritten or deleted, so
an override you wrote yourself is safe.

Two things to know: the launcher is a snapshot, so re-run `install` after an app
update changes it. And if your menu doesn't notice, run `kbuildsycoca6` or log out
and back in.

## Spaces

| Name | Set this in your app |
|---|---|
| `adobe_rgb` | Adobe RGB (1998) |
| `rec2020_g22` | Rec.2020, gamma 2.2 — needs Elle Stone's `Rec2020-elle-V4-g22.icc` |
| `display_p3` | Display P3 |
| `linear_rec709` | linear Rec.709 |
| `linear_rec2020` | linear Rec.2020 |
| `pq_rec2020` / `hlg_rec2020` | PQ / HLG, Rec.2020 primaries |
| `pq_p3` / `hlg_p3` | PQ / HLG, P3 primaries |
| `display` | bypass mode, see below |

`adobe_rgb` is the default when nothing is set on the command line or in the
config file.

If none of those fit, name the two halves yourself using the protocol's own
vocabulary:

```sh
cm-shim -p bt2020 -t pq run mpv
```

`-p` and `-t` always go together, and can't be combined with `-s`. Run
`wayland-info` to see which names your compositor accepts.

The same settings work in `~/.config/cm-shim/config`:

```ini
space = adobe_rgb
intent = relative
```

Command-line flags beat the config file. The color definition is taken whole from
one place or the other, so `-p`/`-t` on the command line replaces a `space =` line
in the file rather than merging with it.

Leave `space` out of the config entirely and you get `adobe_rgb`.

Intents: `perceptual`, `relative`, `relative_bpc`, `absolute`, `saturation`
(default `perceptual`).

On KDE there's one more key, `assume_kde_srgb_is_unmanaged` (default `1`). It's
explained under "KDE with no display profile" in section 3, and ignored
everywhere else.

## About bypass mode

`-s display` claims the app's pixels are already in the compositor's own output
space, so the compositor should leave the surface alone. You load your real
display ICC in the app and let it do everything.

That's the theory. On KWin it doesn't hold up — measured in section 5. It's never
the default; you have to ask for it. It stays in the code because it may behave
correctly on other compositors, but it's not where you should start.

## Troubleshooting

| Symptom | Cause |
|---|---|
| On KDE, looks identical with and without the shim | KDE's display profile is probably set to `None`, so the shim is mirroring sRGB back on purpose. See section 3 |
| Looks identical with and without the shim | The app is on XWayland. Force Wayland with `GDK_BACKEND=wayland` (GTK) or `QT_QPA_PLATFORM=wayland` (Qt) |
| "color management is OFF" notification | The compositor rejected your space or intent. Check `wayland-info` for what it accepts |
| App opens but the shim has no effect | Single-instance app: your launch handed off to a copy that's already running. Close it first, or use the app's flag for this (`gimp --new-instance`) |

---

# 2. Disclaimer

This was written with Fable 5.1, as a proof of concept.

My Rust is rudimentary, and I don't understand the protocol-level details of how
this shim works. I couldn't review this code line by line and tell you it's
correct, and I'm not going to pretend otherwise.

What I have done is verify the part that actually matters: that the light coming
off the monitor is what it should be. I own a colorimeter and I measured it.
Method and numbers are in section 5.

Corrections from people who know Wayland internals are very welcome.

---

# 3. What it does

Every Wayland window carries an implicit claim about what its pixels mean. The
`wp_color_manager_v1` protocol lets an app state that claim out loud. Almost no
apps implement it, so compositors fall back to assuming sRGB.

cm-shim makes the claim on the app's behalf.

## Getting in the middle

`cm-shim run darktable` does three things:

1. Opens a private Wayland socket of its own.
2. Starts darktable with `WAYLAND_DISPLAY` pointing at that socket instead of the
   compositor's.
3. Forwards every message in both directions, using the
   [wl-proxy](https://crates.io/crates/wl-proxy) library.

The app sees what looks like an ordinary compositor. The compositor sees what
looks like an ordinary app. The shim sits in between and adds one thing.

## Hiding the protocol

When the app asks which protocols are available, the shim removes
`wp_color_manager_v1` from the list and binds it for itself, at version 1. The app
never learns it exists, so it can't set a description that contradicts the shim's.

(The older frog color protocol disappears too. wl-proxy doesn't know it, and
anything wl-proxy doesn't know gets filtered out.)

## Declare mode

This is what you get unless you ask for something else. With no `-s`, `-p`/`-t`
and no config file, the shim declares `adobe_rgb` with the perceptual intent, and
prints a notice at startup saying it fell back to that default.

The compositor announces which render intents, primaries and transfer functions it
supports. The shim waits for that list and checks your configuration against it.
If your space or intent isn't there, it stops and goes unmanaged rather than
substituting something close.

If everything checks out, it builds a single image description from two names —
the primaries and the transfer function — and sets that same description on every
surface the app creates. Surfaces created before the description is ready get
queued, and updated the moment it is.

```
[cm-shim] declare: primaries=10 tf=2 (protocol enum values)
[cm-shim] declared description ready (compositor id 32749)
```

## KDE with no display profile

Set the display profile to `None` in KDE and color management does not turn off.
KWin keeps the protocol up, says the output is plain sRGB, and converts
everything into that for a monitor it assumes is sRGB. Declaring Adobe RGB into
that makes your colors worse, not better. Hyprland drops the protocol entirely
in the same situation, which the shim already detects and reports.

So on KDE, declare mode asks the compositor what it prefers for each surface
before declaring anything. The protocol only names the primaries when they're
exactly one of its named sets, so an answer of `srgb` is KWin's own word for its
own state - with a real profile loaded it sends your monitor's measured
chromaticities and no name at all. When the answer is `srgb`, the shim sets that
same description back on the surface instead of declaring. Nothing gets
converted, which is what color management being off actually means.

One line on stderr, once per launch, no notification:

```
[cm-shim] KDE has no display profile set (it prefers plain sRGB); mirroring that back instead of declaring
```

The shim re-asks whenever KWin says its preference changed, so loading a profile
in System Settings, or dragging the window to a monitor that has one, switches it
back to declaring without restarting the app.

This gets one case wrong: a genuinely sRGB monitor with a real sRGB profile
loaded, where KWin says `srgb` and means it. Turn the check off there:

```ini
assume_kde_srgb_is_unmanaged = 0
```

## Bypass mode

For each surface, the shim asks the compositor which image description it prefers
for that surface, sets that description straight back onto the surface, and
releases its handle. It never looks inside it.

If the compositor later signals that its preference changed — you moved the window
to a different monitor, say — the shim asks again and re-applies the answer.

```
[cm-shim] bypass: mirroring the compositor's preferred image description
[cm-shim] surface <- compositor description id 32749
```

That id is the compositor's own handle for the description. You can match it
against the `image description id` that `wayland-info` reports for your output.

## Making it take effect

An image description is pending surface state, so it only applies when the surface
is next committed. An idle app might not commit for a long time. So after setting
a description, the shim marks the surface damaged and commits it itself.

`CM_SHIM_NO_COMMIT=1` turns that off and waits for the app instead.

## What it never does

- Holds no colorimetric values. Space names map to protocol enum numbers, nothing
  else.
- Reads no ICC files.
- Performs no conversion: no matrices, no curves, no math of any kind.

Everything it passes along is either a name the protocol itself defines, or an
object the compositor handed it in the first place. So the shim can't introduce a
color error of its own. Any error you measure belongs to the app or the
compositor.

## When it can't

The shim gives up, visibly, if the compositor has no color management protocol, if
it doesn't support your render intent, if it doesn't support your primaries or
transfer function, or if it rejects the description after the fact.

In all four cases the app still starts, but unmanaged. You get a desktop
notification and a line on stderr, once per launch. Quietly handing you
nearly-right colors would be worse than handing you none.

For the full protocol traffic, run with `WL_PROXY_DEBUG=1`.

---

# 4. Limitations

## You inherit the compositor's math

Declaring a space hands the color conversion over to the compositor. Your app
stops converting and the compositor starts. From that point your colors are only
as good as the compositor's code, and you have very little visibility into it.

- **Gamut clipping is the compositor's call.** Colors your monitor can't reproduce
  get mapped gracefully or clipped flat. You don't get to choose.
- **Render intent is a request, not a guarantee.** Whether relative colorimetric
  behaves differently from perceptual is up to the compositor. Hyprland currently
  advertises only perceptual.
- **Precision is the compositor's call.** Low bit depth or shortcuts in the shader
  will band, and it'll look like your file's fault.
- **It can change under you.** A compositor update can change the math without you
  changing a thing.

Little CMS, which darktable, GIMP and Krita all use, has decades of scrutiny
behind it. Compositor color pipelines are new. Declaring a space is
architecturally the right thing to do, but know what you're trading away, and
measure your own setup instead of assuming.

## Scope

- **Native Wayland only.** Under XWayland the behaviour is undefined, with no
  warning at all.
- **Single-instance apps** hand off to a copy that's already running, which never
  went through the shim.
- **Protocols wl-proxy doesn't know disappear** for the shimmed app. If your app
  depends on something exotic, it quietly loses it.
- **Color-aware apps lose their own color handling**, even in unmanaged mode.
  Don't run them under this. They don't need it.

## The whole window gets declared, not just the image

GTK3 and Qt draw the interface and the image into one surface, so your toolbars
are interpreted in the declared space too.

- Wide gamuts make the UI look oversaturated.
- Linear spaces wash it out completely.
- Menus, tooltips and cursors are separate surfaces and get declared as well.
  Excluding them was considered, not implemented.
- Buffers are 8-bit regardless of your monitor's depth. Wide spaces get coarser
  steps between values, and linear spaces band visibly.

## Compositor gaps

**KWin**

- Doesn't turn color management off with the display profile set to `None`; it
  claims sRGB and converts to it. Worked around, see section 3.
- Won't take ICC files from clients.
- No custom power curve.
- Transfer functions limited to `gamma22`, `pq`, `ext_linear` and `bt1886`, which
  means standard Display P3 (sRGB curve), HLG and ProPhoto can't be declared there
  at all.

**Hyprland**

- Advertises only the perceptual intent.
- With an ICC loaded, appears to clip color-managed content to sRGB.
- Reports a D50 white point for the output.

**Both:** the shim binds protocol version 1, so names that only exist in version 2
are unavailable.

## Spaces

- `rec2020_g22` needs an external ICC loaded in the app. It isn't bundled with
  darktable.
- No trusted P3 gamma 2.2 profile was found, so that combination isn't offered.
- sRGB isn't offered either. It's what the compositor already assumes, so if
  that's your space you don't need the shim.
- darktable's Rec709 RGB uses the Rec.709 camera curve rather than BT.1886, and
  ProPhoto has no named primaries. Neither has an exact protocol match.
- **Multi-monitor is a real limitation.** A surface carries one description and
  the app knows one display profile, so only one screen can be correct at a time.

## Launchers

- The override is a snapshot. Re-run `install` after an app update.
- Menus cache their entries.
- Flatpak apps are untested.
- Launching from a terminal, or through a file association that skips the desktop
  launcher, bypasses the shim.
- Launchers hold the absolute path of the binary, so they go stale if it moves.
  `install.sh` flags this when it spots it.
- Notifications need `notify-send` and a running notification daemon.

---

# 5. Verification

## What I measured with

A Calibrite Display Pro HL colorimeter, read with `spotread` from ArgyllCMS. It
reports the actual light coming off the screen. It doesn't care what any piece of
software claims is happening.

## How to read the numbers

Results are in dE2000: one number for how far apart two colors are, scaled to
human vision.

| dE2000 | What it means |
|---|---|
| under 0.5 | Identical. Nobody can see this. |
| around 1.0 | Roughly the threshold for spotting a difference side by side. |
| 2 – 3 | Visible if you're looking for it. |
| over 3 | Obvious. |

So dE 0.1 doesn't mean "close enough". It means the difference is far below
anything a person could detect.

## The test

The question is whether inserting the shim changes the colors. It shouldn't.

I measured the same patches in two configurations:

1. **Reference.** The app converts straight to the real display profile, with the
   compositor's color management switched off. This is the known-good path people
   have used for years.
2. **Through cm-shim.** The app converts to an intermediate space, the shim
   declares that space, and the compositor finishes the conversion to the display.

If the two measure the same, the extra hop through the compositor costs nothing.

## Declare mode, KWin

| Comparison | dE2000 |
|---|---|
| Reference vs `adobe_rgb` | ≈ 0.1 |
| Reference vs `rec2020_g22` | ≈ 0.1 |
| Same test, nomacs image viewer | ≈ 1.4 |

0.1 is as close to "no change at all" as this measurement can resolve. Declare
mode works.

nomacs is the outlier and I haven't worked out why yet. It only shows up with that
one app, which points at how nomacs handles its own profile rather than at the
shim — but that's a hypothesis, not a finding. It's on the list.

## Bypass mode, KWin

| Comparison | dE2000 |
|---|---|
| Reference vs bypass | 1.16, with a luminance shift |

This is a negative result and I'm reporting it as one. Bypass is supposed to mean
"don't touch this surface", so a true passthrough should measure the same as the
reference. It doesn't. KWin is still applying part of its ICC profile after
compositing, despite being told the surface is already in its preferred space.

So on KWin: put the profile in the app, or in KWin, not both. Bypass is on the
back burner there. It may still be correct on other compositors, which is why the
code stays.

## Checking it yourself, without an instrument

This won't catch a subtle error, but it will catch a double conversion, and a
double conversion is the failure that ruins images.

1. Open a neutral gray ramp and a saturated test image in the app, started
   normally. Set the app's monitor profile to your real display ICC.
2. Open a second copy through `cm-shim run <app>`, same settings, side by side.
3. Toggle the ICC profile in your compositor's display settings and watch.

If bypass is a true passthrough, the shimmed window should look the same whether
the compositor's profile is loaded or not. If it visibly shifts when you toggle,
the compositor is still applying that profile after compositing — which is exactly
what the measurement above found on KWin.

## Still ongoing

I'm continuing to test color management approaches under Wayland generally, not
just this shim. So far, letting apps manage color themselves with KDE's and
Hyprland's own color management switched off has given promising results. A proper
write-up will follow once I've tested it further.
