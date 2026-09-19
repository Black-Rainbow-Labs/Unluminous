# Building an installer for Unluminous

Everything that turns the built binary into something a person can install. One folder, two
platforms, one drawing of the icon. `tasks/unluminous-installer-tdd.md` says why each of these is what it
is; this says how to run them.

```
installer/
  icon/       the Unluminous mark: a program that draws it, and the drawn files
  windows/    the Inno Setup script, and the script that builds and installs it
  macos/      the bundle's manifest, and the script that builds Unluminous.app and the disk image
  dist/       what the two build scripts write. Not committed.
```

Both installers ship **two** programs: `unluminous`, the editor, and `unluminous-cli`, which drives a running
one from a terminal. They go into one folder on purpose — `unluminous-cli` looks for `unluminous` beside itself
— so on Windows the PATH task puts both on the path together, and on macOS they are both inside
`Unluminous.app/Contents/MacOS`, which is one symlink away from the path:

```sh
ln -sf /Applications/Unluminous.app/Contents/MacOS/unluminous-cli /usr/local/bin/unluminous-cli
ln -sf /Applications/Unluminous.app/Contents/MacOS/unluminous     /usr/local/bin/unluminous
```

`unluminous-cli/README.md` says what it does. On macOS `unluminous-cli` is signed on its own before the bundle
is signed round it, because codesign treats a second binary inside `Contents/MacOS` as something that
must carry its own signature.

The version comes from `Cargo.toml` and nowhere else. It reaches `unluminous.exe`'s version block, the
installer's file name, the Add or Remove Programs entry and `Unluminous.app`'s `Info.plist` from there, so
releasing a new version is changing one number.

**Releasing is one command, and it is not this one.** `pwsh tools/release.ps1` bumps that number,
runs the Windows script below with `-Install`, tags, pushes and publishes the GitHub release with the
installer attached. The scripts here are what it drives, and are what to run when you want an
installer without a release. `CLAUDE.md` records the rule that a finished task ends with a release.

---

## Windows

```powershell
powershell -File installer\windows\build.ps1              # build the installer
powershell -File installer\windows\build.ps1 -Install     # build it and install it on this machine
powershell -File installer\windows\build.ps1 -Install -AllUsers   # into Program Files, needs admin
powershell -File installer\windows\build.ps1 -SkipBuild   # use target\release\unluminous.exe as it is
powershell -File installer\windows\build.ps1 -Icon        # redraw the icon first
```

It writes `installer\dist\UnluminousSetup-<version>-x64.exe`, about 6.5 MB.

The script installs the Inno Setup compiler with `winget` the first time, if it is not already there.
Nothing else is needed beyond what building Unluminous already needs: `rc.exe`, which puts the icon inside
`unluminous.exe`, comes with the Windows SDK that the MSVC toolchain already depends on.

**A `CC` in the environment stops the build, and `tools/release.ps1` unsets it.** `cc-rs` takes a `CC`
it is given as the whole answer and skips its own Visual Studio lookup, so with a user `CC` naming
`cl.exe` by its full path and no `INCLUDE` or `LIB` beside it, the compiler it invokes cannot find a
single system header and `libsqlite3-sys` fails to build — from any ordinary shell, which is every
shell a release is started from. `task-1984` T4 measured that on this machine: the only gate failed at
step 0 and every number in that review had to be taken with `env -u CC`. The release scripts and
`tools/nightly.ps1` now run cargo with `CC` unset. Nothing puts the variable back or takes it away —
it is the person's — so a `cargo build` typed by hand in that shell still fails, and `env -u CC cargo
build` is what to type instead.

**What the installer does.** A plain double click installs into `%LOCALAPPDATA%\Programs\Unluminous` with
no elevation prompt; the first page offers all users, which puts it in `Program Files` instead. Five
optional things, all on one page and all remembered by the uninstaller:

| | |
|---|---|
| Desktop icon | an **Unluminous** shortcut on the desktop |
| PATH | `unluminous` opens a folder from any terminal |
| Right click a file | *Open with Unluminous* |
| Right click a folder | *Open with Unluminous*, on the folder and on the empty space inside it |
| Open with | Unluminous is offered for `.md .markdown .txt .rs .js .ts .json .toml .yml .yaml` — offered, never taken as the default |

**Uninstalling** removes the files, the shortcuts, the `PATH` entry and every registry key, and leaves
every other `PATH` entry exactly as it was. It deliberately does **not** touch `%APPDATA%\Unluminous` —
the settings, the pane sizes, the recent projects and any installed plugins — because uninstalling to
install a newer version must not throw those away.

**Installing over a running Unluminous.** Unluminous does not answer the Restart Manager's request to shut down,
so setup cannot close it by itself: a person is shown the list and closes Unluminous, and `build.ps1
-Install` closes it for you, with the window's own close rather than a kill. See section 4 of the TDD.

---

## macOS

```bash
installer/macos/build.sh              # Unluminous.app in installer/dist, unluminous-<version>.dmg in releases
installer/macos/build.sh --install    # and copy the bundle into /Applications
installer/macos/build.sh --no-dmg     # just the bundle
installer/macos/build.sh --icon       # redraw the icon first
installer/macos/build.sh --notarize   # sign, send the image to Apple, staple the ticket to it
```

It uses nothing that is not in a stock Xcode command line install: `cargo`, `lipo`, `iconutil`,
`codesign`, `hdiutil`, `plutil`, and `notarytool` and `stapler` when notarising. For a universal binary,
have both targets installed first:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

With only one installed it builds that one and says so.

The image goes to `releases/unluminous-<version>.dmg`. `installer/dist/` is the working area and is
rewritten on every run.

### macOS, from Windows

`build.sh` needs a Mac. `installer/macos/build-on-windows.ps1` does the same job on the Windows
machine (task-1995), because every Apple program it used has a replacement that runs there: zig links
the Mach-O, and `rcodesign` replaces `lipo`, `codesign`, `notarytool` and `stapler`. The committed
`installer/icon/unluminous.icns` is the icon, so `iconutil` is not needed either.

```powershell
pwsh tools\cross\fetch-toolchain.ps1                       # once: zig, cargo-zigbuild, rcodesign
pwsh installer\macos\build-on-windows.ps1 -CliOnly         # unluminous-cli, which needs no SDK
pwsh installer\macos\build-on-windows.ps1 -Sdk <MacOSX.sdk> -Notarize
pwsh tools\release.ps1 -Macos                               # the whole release, with the bundle attached
```

**It needs a copy of Apple's macOS SDK, and that is the one thing the machine cannot supply itself.**
Unluminous is a windowed application: the link line asks for `-lobjc` and for AppKit, Metal, QuartzCore,
WebKit, Carbon, ApplicationServices, CoreGraphics, CoreVideo, Foundation, CoreFoundation and
Security. zig ships a stub for `libSystem` and for nothing else, so the link stops at the first of
them with `unable to find dynamic system library 'objc'`. The stubs are in Xcode and in the Command
Line Tools, at `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk`; copy that directory to
`tools/cross/sdk/MacOSX.sdk`, or name it with `-Sdk` or `$env:SDKROOT`. **Apple's licence for the SDK
says Apple-branded hardware**, so whether a copy belongs on a Windows machine is a decision for
whoever accepted that licence; the script never downloads one. `unluminous-cli` links no framework and
cross compiles with no SDK at all.

**The identity is a `.p12` rather than a keychain entry**, because Windows has no keychain.
Keychain Access on a Mac exports a Developer ID Application identity as one. `CODESIGN_P12` names it
in the same `installer/macos/notarize.env` the rest of the credentials live in, and
`CODESIGN_P12_PASSWORD_FILE` holds its password sealed with DPAPI — which encrypts under one Windows
account, so the file is worthless on another machine. The notary credentials are unchanged:
`NOTARY_KEY`, `NOTARY_KEY_ID` and `NOTARY_ISSUER`, folded into the one JSON file rcodesign wants on
the RAM disk for the length of the submission.

**A `.p12` without a Mac to export one from.** `C:/jason/dev/inillucent/packaging/macos/new-apple-csr.ps1`
obtains a Developer ID from a browser, and leaves it as a private key sealed with DPAPI beside the
`.cer` Apple issued. That is the same identity in a different container, and
`installer/macos/import-developer-id.ps1` converts it:

```powershell
pwsh installer\macos\import-developer-id.ps1
```

It writes `%LOCALAPPDATA%\unluminous\apple\developer-id-application.p12`, seals a random password
beside it, and prints the two `notarize.env` lines. The `.p12` goes outside the repository because
`.gitignore` covers `notarize.env` and `*.p8` and matches no `.p12`, and a Developer ID private key
committed to a public repository cannot be taken back. It tells openssl to use the older PKCS#12
algorithms: openssl 3 defaults to AES-256-CBC, which rcodesign rejects with `incorrect password given
when decrypting PFX data` — a message about the password, not the cipher, so the obvious next move is
to retype a password that was never wrong.

**It writes a `.zip`, not a `.dmg`.** A disk image holds an HFS+ filesystem and writing one needs
`hdiutil`; rcodesign signs a disk image but does not create one. Apple's notary accepts a zipped
bundle as well, and the ticket is stapled to the **application** inside it rather than to the zip, so
what a person ends up running carries its own ticket and opens with no network — the same property
the two-submission order below gives the image. `build.sh` still writes the `.dmg` on a Mac.

**What it cannot check.** A Mach-O only runs on macOS, so nothing on Windows can say the application
starts, that `spctl` accepts it, or that it behaves under the hardened runtime. Apple's notary service
covers part of it: it unpacks the submission, walks every Mach-O and refuses an unsigned binary, a
missing hardened runtime, a missing timestamp or an archive it cannot parse. The script prints what it
checked and what it could not, every time.

### Signing, in three levels

The script says which one it did, and checks with `spctl` afterwards rather than leaving it to be
guessed at.

| | What it takes | What a person who downloads it sees |
|---|---|---|
| **Ad-hoc** | nothing | "Apple cannot check it for malicious software", and a right click and *Open* gets past it, once. A copy built on the machine it runs on has no quarantine flag and opens with no warning at all. |
| **Developer ID** | `CODESIGN_IDENTITY` naming an identity in the keychain | The same warning. A signature alone is not enough on macOS 10.15 and later; the notarisation is what removes it. |
| **Notarised** | that, plus `--notarize` and credentials | Nothing. It opens. |

The bundle is signed with the **hardened runtime** at every level, including ad-hoc. Notarising requires
it, and an application that breaks under it breaks whether the signature is real or not, so the ad-hoc
build is where that shows up rather than the first signed one. Unluminous runs under it: no entitlements are
needed, because it loads only system frameworks and the processes it starts — `git`, a shell — are not
restricted by the runtime.

### Notarising

The identity and the credentials are written down once, in a file the script reads:

```bash
cp installer/macos/notarize.env.example installer/macos/notarize.env
# fill it in, then
installer/macos/build.sh --notarize
```

`notarize.env` is ignored by git — the API key it points at is a credential and the identity and the
issuer are account details. Anything already in the environment wins over it, so a one-off run can still
override a value. Nothing is printed or written by the script either way.

**Two submissions, and the order is the point.** A notarisation ticket has to be stapled to the thing it
covers, and what a person ends up running is the application they dragged out of the image. So the
application is notarised and stapled *first*, and only then is the image built round it and notarised in
its turn. Doing the image alone leaves `Unluminous.app does not have a ticket stapled to it`, which works only
while the machine can reach Apple; a stapled ticket is checked locally, so both work with no network.

Each submission takes a minute or two. Afterwards the script asks `stapler validate` and `spctl` whether
it took, and fails if they disagree, so a green run means a person downloading the image sees no warning.

### What is needed before any of that works

None of it can be done from a checkout alone, and each one is a thing only the account holder can do:

1. **A paid Apple Developer Program membership.** A free Apple ID cannot have a Developer ID
   Application certificate, which is the only kind of signature Gatekeeper accepts outside the App
   Store.
2. **The certificate, in the login keychain.** Xcode: *Settings*, *Accounts*, sign in, select the team,
   *Manage Certificates*, the `+`, *Developer ID Application*. It needs the Account Holder or Admin role.
   Signing in to Xcode on its own is not enough: that produces an `Apple Development` certificate, which
   is for running on your own machines. Signing with one of those and asking `spctl` gives `rejected`
   exactly as ad-hoc does, and Apple refuses to notarise it.
3. **The Team ID**, ten characters, from developer.apple.com under Membership, and also the part in
   brackets in the identity's name.
4. **Credentials for `notarytool`**, either of:
   - an **App Store Connect API key**: appstoreconnect.apple.com, *Users and Access*, *Integrations*,
     *App Store Connect API*, `+`, role Developer. Download the `AuthKey_<keyid>.p8` — one chance only —
     and note the Key ID in its row and the Issuer ID above the table.
   - an **app-specific password**: appleid.apple.com, *Sign-In and Security*, *App-Specific Passwords*.
     The Apple ID's own password is refused, and that section only exists on an account with two-factor
     authentication turned on.

`security find-identity -v -p codesigning` lists what the machine actually has. Until step 2 is done it
says `0 valid identities found`, and the script stops with that same list rather than producing
something that looks signed and is not.

**What has been run.** Both sides now. The Windows side was built, installed, run, uninstalled and
reinstalled on a real machine. The macOS side was run on an Apple silicon Mac on 2026-08-25 and needed
no correction: `build.sh` went from a clean checkout to `Unluminous.app` and `Unluminous-0.1.0.dmg` on the first
run, the icon was built by `iconutil`, and the two places the design document expected to want a fix,
the `rustup target list` check and the ad-hoc `codesign --deep` call, both behaved as written.

What was checked afterwards:

| | |
|---|---|
| The bundle | `Contents/MacOS/unluminous`, `Contents/Resources/Unluminous.icns`, `Contents/Info.plist`, `Contents/PkgInfo` and `_CodeSignature`, 15 MB |
| The manifest | `plutil -lint` passes, and the version, `CFBundleName` and `com.jasonmcaffee.unluminous` are all substituted in |
| What the system calls it | `mdls` reports the display name `Unluminous` and the version `0.1.0`, which is what the Dock and the menu bar read |
| The icon | a real 1024 point `icns`, by `sips` |
| The signature | `codesign --verify --deep --strict` passes on the bundle, on the copy in `/Applications` and on the copy inside the mounted image |
| The disk image | 6.7 MB, mounts, holds `Unluminous.app` beside an alias to `/Applications` |
| Installing | `--install` puts it in `/Applications`, `open -a` launches it, and the folder passed after `--args` reaches the window: it turns up at the top of `recent.txt` |
| Installing over a running copy | works, because a Mac replaces the bundle and leaves the running process on its old inode. There is nothing here like the Restart Manager problem the Windows side has |

**Signed and notarised, as of 2026-08-25.** `Developer ID Application: Jason McAffee (A7L778FFNA)`, the
hardened runtime, a secure timestamp, and both submissions accepted by Apple. Checked the way a person
receiving the image is: the quarantine flag was set on a copy of the `.dmg` by hand, and

```
spctl --assess --type open --context context:primary-signature  →  accepted, source=Notarized Developer ID
```

for the image, the same for `Unluminous.app` inside it, and `stapler validate` passes on the image, on the
application in the image, and on the copy installed in `/Applications`. So a downloaded copy opens with
no warning and with no network.

Two smaller things left as they are, deliberately. The binary is **arm64 only** on a machine with only
that target installed — `rustup target add x86_64-apple-darwin` and another run makes it universal, and
the script says which it built. And the disk image opens as a plain Finder window rather than one with
the icons positioned and a background picture: laying that out means driving Finder with AppleScript,
which is a permission prompt and a fragile step for something that is decoration.

---

## The icon

```bash
cargo run --release --manifest-path installer/icon/Cargo.toml
```

Draws the mark once at 1024 points and writes `unluminous.ico`, `unluminous.icns` and `Unluminous.iconset/` beside
itself. It is a workspace of its own, so `cargo build` on Unluminous does not build it, and its output is
committed — `unluminous.ico` is a build input for `unluminous.exe`, so a fresh checkout has to have it already.

Change the drawing in `installer/icon/src/main.rs`, run it, and **look at the pictures** before
committing them, the same rule the screenshot tests are held to. The colours are the ones in
`theme::color`; a colour that is not in that list does not belong in the icon either.
