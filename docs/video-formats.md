# Video formats

What **Convert and resize** can turn a video into. The work is done by [FFmpeg](https://ffmpeg.org): the system's one when there is one, otherwise FFmpeg 9.0 (from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds), GPL) downloaded once by EyeCanSee into its data folder. The first time the window shows videos, EyeCanSee tries every encoder below on a few black frames and lists only those that work on that computer: an encoder for a graphics card that is not there, or whose driver is missing, does not appear.

## Containers

| Container | Extension | Codecs | Sound |
|---|---|---|---|
| MP4 | mp4 | H.264, HEVC, AV1, VP9, MPEG-4 | AAC |
| MKV | mkv | H.264, HEVC, AV1, VP9, VP8, MPEG-4, ProRes | Opus (AAC if Opus is missing) |
| WebM | webm | AV1, VP9, VP8 | Opus (Vorbis if Opus is missing) |
| MOV | mov | H.264, HEVC, ProRes | AAC |
| AVI | avi | MPEG-4, H.264 | MP3 (AC-3 if MP3 is missing) |
| GIF | gif | GIF | none |

## Codecs

| Codec | Plays on | Size at the same quality |
|---|---|---|
| H.264 | everything: phones, TVs, browsers, old computers | reference |
| HEVC (H.265) | phones, TVs and computers of the last years | about half of H.264 |
| AV1 | recent browsers, phones and TVs; royalty free | the smallest |
| VP9 | browsers (YouTube uses it); royalty free | between H.264 and AV1 |
| VP8 | old browsers | larger than VP9 |
| MPEG-4 Part 2 | old players (DivX, Xvid) | large |
| ProRes | editing programs (Final Cut, DaVinci Resolve) | very large, best quality |
| GIF | everything that shows images; no sound, 256 colours | very large |

## Encoders

The list in the window shows the graphics card ones first. A graphics card is much faster and leaves the processor free; at the same quality its files are a little larger than those of the best processor encoders.

| Codec | Graphics card | Processor |
|---|---|---|
| H.264 | NVIDIA (`h264_nvenc`), Intel Quick Sync (`h264_qsv`), AMD (`h264_amf`), Windows Media Foundation (`h264_mf`), VA-API on Linux (`h264_vaapi`) | x264 (`libx264`), OpenH264 (`libopenh264`) |
| HEVC | `hevc_nvenc`, `hevc_qsv`, `hevc_amf`, `hevc_mf`, `hevc_vaapi` | x265 (`libx265`) |
| AV1 | `av1_nvenc` (RTX 40 and later), `av1_qsv`, `av1_amf`, `av1_mf`, `av1_vaapi` | SVT-AV1 (`libsvtav1`), libaom (`libaom-av1`), rav1e (`librav1e`) |
| VP9 | `vp9_qsv`, `vp9_vaapi` | libvpx (`libvpx-vp9`) |
| VP8 | | libvpx (`libvpx`) |
| MPEG-4 | | FFmpeg's own (`mpeg4`) |
| ProRes | | FFmpeg's own (`prores_ks`) |
| GIF | | FFmpeg's own (`gif`) |

Processor encoders, from the fastest to the slowest at their settings in EyeCanSee: SVT-AV1 and x264, then x265 and libvpx, then libaom. VA-API uses the first graphics device (`/dev/dri/renderD128`): on a computer with two graphics cards it may be the one without VA-API encoding.

## Quality

Three levels, set for each encoder:

| Encoder | High | Medium | Small file |
|---|---|---|---|
| x264, x265 | CRF 18 | CRF 23 | CRF 28 |
| SVT-AV1, libaom, libvpx (VP9) | CRF 24 | CRF 32 | CRF 40 |
| NVIDIA | CQ 19 | CQ 25 | CQ 31 |
| Intel Quick Sync, VA-API, AMD | QP 20 | QP 25 | QP 30 |
| Windows Media Foundation | quality 80 | quality 65 | quality 50 |
| MPEG-4 | q 3 | q 5 | q 8 |
| ProRes | 422 HQ | 422 | Proxy |
| Sound (AAC, Opus, MP3…) | 192 kbit/s | 128 kbit/s | 96 kbit/s |

H.264 from x264, OpenH264 and MPEG-4 is written with 8 bits per colour, so that every player opens it, also when the original is a 10-bit HDR video from a phone.

## Other choices

- **Turn and flip** and **Size** are the same as for images. A phone video recorded sideways is first shown upright, as the phone meant it, then turned. The sides stay even numbers, as most codecs want.
- **Keep the metadata**: date, place and camera of the original are copied into the new file.
- A video on a phone or a server is copied into a temporary file first; a destination there gets the new file at the end.
