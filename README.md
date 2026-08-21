# wall

Random wallpaper setter for Android via Termux. Sets the same image on both home and lock screen. Cycles through every wallpaper before repeating.

## Requirements

- [Termux](https://f-droid.org/packages/com.termux/)
- [Termux:API](https://f-droid.org/packages/com.termux.api/) app installed
- `termux-api` package: `pkg install termux-api`

## Install

1. Download `wall` from [Releases](../../releases/latest)
2. Move it into Termux and make it executable:

```sh
# from Termux
cp /sdcard/Download/wall ~/bin/wall
chmod +x ~/bin/wall
```

> If `~/bin` doesn't exist: `mkdir -p ~/bin` — it's already on `$PATH` in Termux.

3. Run it once to set up:

```sh
wall
```

You'll be asked three things — screen width, screen height, and your wallpaper folder. Answers are saved to `~/.cache/wall_cfg` and never asked again.

## Usage

```sh
wall
```

To run on a schedule, use termux-job-scheduler or add a cron job:

```sh
# every 1 hour
crontab -e
0 * * * * ~/bin/wall
```

## How it works

- Shuffles all wallpapers in your folder, cycles through them in order before reshuffling.
- Scales each image to cover the screen exactly (no black bars, no stretching) using Lanczos3.
- Writes one temp file at `~/.cache/wall_tmp.png` so home and lock screen get the identical image.
