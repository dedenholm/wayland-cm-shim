**The problem:**  Under color managed wayland, any app that doesnt speak waylands color management protocol is assumed to be sRGB. A non sRGB color space set in an application that doesn't use waylands cm protocol causes color space transforms to be applied twice, resulting in undersaturated colors. So you can choose between your colors being limited to sRGB, or wrong.

**My solution:** cm-shim sits between the application and the compositor and lets you tell the compositor which color space your app is actually using. This lets you for example use darktable with a wide color gamut under (some) wayland compositors.

This seem to work well under KDE Plasma.
hyprland clamps all apps to sRGB, when you use an icc profile, so the usefulness of  cm-shim is limited. It can help your apps keep correct sRGB colors under hyprland, while setting up apps for wide gamut work in other DEs/sessions. 

I've detailed some approaches to get correct wide gamut colors under hyprland in the setup section of this readme.

### The simplest way to use cm-shim:

```sh
cm-shim run darktable
```

this will tell your compositor that darktable is using adobe_rgb. set adobeRGB as your display profile in darktable and you have the first part of a color managed pipeline.
## Install:
### Dependencies:

You will need:
Rust -- for building, 
notify-send lets cm-shim warn if color-management fails.
wayland-info for cm-shim to check what color spaces your compositor supports.

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


## Build, Install and Uninstall

```sh
git clone <this repository>
cd cm-shim
./install.sh
```

The install script puts the binary in `~/.local/bin/cm-shim`. If that isn't on your `PATH`, the
script asks if you want to put it in `/usr/local/bin` instead. For this, it will ask for `sudo` for that one copy.

It also writes a fully commented config to `~/.config/cm-shim/config`. Everything
in it is commented out, so it changes nothing until you edit it.
Any flags 
### To remove the binary and every launcher cm-shim made:

```sh
./install.sh uninstall
```

Your config file will not be removed

If your build fails you can check your rust version:

```sh
rustc --version
```

cm-shim needs **1.70 or newer**

If you're somehow stuck below 1.70, get Rust from rustup instead of your package manager:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

# Setup:

**A short summary of how this works:**
An application renders its output to a color space that you set (e.g Adobe RGB) --> cm-shim tells the compositor what color space the window is rendering in --> the compositor uses this information to transform the application working space, to the color space of your monitor.

### Step 1: **Create a VCGT-less .icc profile with DisplayCAL**
For any of this to make sense you have to have already made a .icc color profile for your monitor, *without* a vcgt table. vcgt is a gamma correction that is set in your gpu hardware. At the moment Wayland doesnt have any way to apply this, so you will have to make a profile that doesn't rely on outsourcing those corrections to the gpu.

 [Xaver Hugl has a great explainer for how to create a vcgt less icc file on his blog.][https://zamundaaa.github.io/wayland/2024/07/16/how-to-profile.html]. 
 his tutorial also works well under hyprland, if you set cm__enabled = false in your config.

```lua
render = {
    cm_enabled = false,
}
```

### Step 2: **Set your compositor color profile to your monitors VCGT-less .icc**
In KDE: System settings > Display Configuration > Color profile;


In Hyprland:
```lua
hl.monitor({
    output      = "DP-1",
    mode        = "2560x1440@360",
    position    = "0x0",
    scale       = 1,
    bitdepth    = 10,
    vrr         = 0,
	icc         = "/absolute/path/to/your/.icc"
	)}
```

### Step 3: **open your application through cm-shim to the color space of your choice, or leave it at default; AdobeRGB**

run your application with
```
cm-shim run <your application>
```

cm-shim defaults to adobe-rgb. you can choose the intermediate color space you want to use with the -s option. you can find what color spaces are available in ~/.config/cm-shim/config

```
cm-shim -s rec2020_g22 run <your application>
```

For wider gamut monitors, like QD-OLED, setting cm-shim to rec2020_g22 and using [rec2020-elle-V4-g22.icc from elles well behaved profiles][https://github.com/ellelstone/elles_icc_profiles/blob/master/profiles/Rec2020-elle-V4-g22.icc] as your application output space seems to work well. 
In theory profiles with this large gamut might create some banding in applications that output color in 8 bit. I've yet to experience any banding myself.
### Step 4: **Set your application color space to the same color space you chose in step 3**
IF COLOR SPACE SET IN cm-shim AND YOUR APPLICATION DO NOT MATCH, COLORS **WILL** BE INNACURATE, WITHOUT ANY WARNING.

For colors to be correct when using cm-shim, the two settings have to be matching; what color space the app is outputting, and what the compositor expects the app to output. You set the color space in cm-shim, and set the same color space in the application.

you can override cm-shims default in ~/.config/cm-shim/config
any flag passed to cm-shim run or cm-shim install overrides the config.

### Step 5: **Make it persistent**

```sh
cm-shim install <your application>
```

cm-shim `install` copies the applications .desktop file from  /usr/share/applications, into `~/.local/share/applications/`, and changes any `Exec=` line to route the application through the shim.

cm-shim tries to match`<your application>` against the desktop file name (org.gimp.GIMP), a part of it, or the program name. If several match, the shim lists them and changes nothing.

with install you can use the same flags to set your preferred color space:

```sh
cm-shim -s rec2020_g22 install <your application>
```
 
 launching the application directly from the terminal bypasses the shim

to remove cm-shim from an application:

```sh
cm-shim uninstall <your application>
```

cm-shim install and uninstall will refuse to touch any .desktop file that doesn't carry a `# cm-shim override of /usr/share/applications/` tag. this is to make sure it doesn't mess with any overrides you have created yourself. 


# AI Disclaimer

This was entirely with Fable 5.1, mostly as a proof of concept, but it seems to be working better than i expected.

My knowledge of Rust is pretty basic, and I have no idea of how Wayland protocols actually work in detail, or how this shim really works under the hood. Im not proficient enough to review this code line by line, but i have a colorimeter and have tried to verify that this shim behaves as expected, to the best of my ability.

The delta E of darktable, gimp and firefox

Corrections from people who know Wayland internals are very welcome. If you are proficient in rust and find this tool useful, maybe you'wd want to maintain it? i would gfladly hand this project over. Its about 700 lines of code.
