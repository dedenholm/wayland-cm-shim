# Simplified Instructions

### 1: Download and install.
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


Download the repo and run the install script

```sh
git clone https://github.com/dedenholm/wayland-cm-shim/
cd wayland-cm-shim
./install.sh
```


### 2: **Create a VCGT-less .icc profile with DisplayCAL**
 [Follow this tutorial on Xaver Hugl's blog.](https://zamundaaa.github.io/wayland/2024/07/16/how-to-profile.html). 

### 3: **Set your compositor color profile to the .icc created in step 2**
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
### 3: Setup cm-shim to intercept your application:
```sh
cm-shim install <your application>
```

### 4: Set your application color space to adobeRGB
