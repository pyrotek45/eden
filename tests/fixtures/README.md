`trim-source.ogg` is a synthetic 0.6-second, 44.1 kHz stereo sine for cold-source
trim-bound tests. It contains no recorded or third-party audio. Generated with:

```sh
ffmpeg -f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.6 \
  -ac 2 -c:a libvorbis -q:a 2 -map_metadata -1 trim-source.ogg
```

Tests read the committed fixture; FFmpeg is not a test/runtime dependency.
