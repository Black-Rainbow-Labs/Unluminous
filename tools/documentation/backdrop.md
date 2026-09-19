# Making the backdrop again

`backdrop.jpg` is the thing every picture in `documentation/overview.md` and
`documentation/database.md` is composited over. It is generated on this machine rather than drawn by
hand, which is how the bundled plugin icons are made — `crates/unluminous-app/plugins/*/icon.md` each
record the same recipe. Written down here so it can be made again without guessing.

**Generated rather than borrowed.** A wallpaper taken off a machine is somebody else's picture, and a
gallery in a public repository cannot carry one whose licence nobody can name.

## What it has to be

Not a gradient. What the gallery exists to show is that the colour in the editing area is the thing
behind the window rather than a shade somebody chose, and that only reads when the thing behind has
real variation in it — light, colour and structure that visibly continues underneath the window and
out past its edge. It also has to be **dark**, because the window is, and a bright backdrop behind a
translucent dark editor reads as a fault rather than as a desktop.

## 1. Render it

Through the AI service's `POST /image-creation/generateImageToProjectFile`, which renders with Krea 2
and writes a verified PNG straight into this repository. It needs the local tooling token, which
every agent terminal has.

```bash
curl -s -X POST http://localhost:8091/image-creation/generateImageToProjectFile \
  -H 'Content-Type: application/json' -H "x-skip-token: $CLAUDE_SKIP_TOKEN" \
  -d '{
    "prompt": "A dark abstract desktop wallpaper. A deep indigo and near-black night sky filling most of the frame, with a broad soft aurora of teal, violet and warm amber light sweeping diagonally from the lower left to the upper right, its edges diffuse and glowing. Below it a range of dark layered mountain silhouettes in receding tones of blue-grey, and a still lake reflecting the aurora in soft broken bands. A scatter of small stars in the upper right. Painterly, smooth, cinematic, high dynamic range, rich saturated colour against deep shadow, no harsh detail, no text.",
    "negativePrompt": "text, letters, words, numbers, watermark, signature, logo, user interface, window, icons, desktop icons, taskbar, screenshot, people, person, face, animal, building, city, busy, cluttered, noisy, grainy, low contrast, flat grey, washed out, pale, white background",
    "width": 2560, "height": 1440,
    "projectId": "unluminous",
    "relativePath": "_agent_output/task-1994-documentation/backdrop-source.png",
    "timeoutMs": 900000
  }'
```

Two things about that call that are easy to get wrong:

- **The negative prompt says `user interface`, `window` and `screenshot`.** Asking for a desktop
  wallpaper otherwise produces a picture of a desktop with windows on it, and a picture of a window
  behind a window is not what any of this is for.
- **The width is a request rather than an instruction.** 2560 was asked for and 2048 by 1440 came
  back, which is larger than the 1896 by 1256 a composite needs and is therefore fine. Check the
  answer's `width` and `height` rather than assuming.

## 2. Turn it into the plate

```powershell
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile('_agent_output/task-1994-documentation/backdrop-source.png')
$codec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$parameters = New-Object System.Drawing.Imaging.EncoderParameters(1)
$parameters.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter([System.Drawing.Imaging.Encoder]::Quality, [long]94)
$source.Save('tools/documentation/backdrop.jpg', $codec, $parameters)
$source.Dispose()
```

JPEG at 94 rather than the PNG, because the plate is composited under a window and then written out as
JPEG anyway: 260 KB against 2 MB, and nothing of the difference survives the second encoding.

## 3. Look at it

Take one picture and open it.

```powershell
pwsh tools/documentation/capture.ps1 -Only 01-unluminous-window
```

What to check is not the plate on its own. It is whether the aurora is visible **through** the editing
area and whether the mountains continue under the window and out into the margin. A plate that looks
good alone and disappears behind the window has failed at the one thing it is for.

## And the picture in the fixture

`fixture.ps1` crops a region of this same plate into `images/aurora.jpg` for the tab that shows a
picture. It is a crop rather than the whole thing so that the picture in the tab and the thing behind
the window are not the same image, which reads as a fault rather than as a photograph.
