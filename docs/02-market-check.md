# 02 — Market Check: Squash (cross-platform OSS file compressor)

Date: 2026-08-13. Method: `app-market-research` skill, Path 1 (validate) + Path 2 (gaps) + Path 3 (keywords), adapted for desktop OSS — AppKittie app-store data not applicable; web sources + GitHub API used instead. All numbers fetched live on this date.

## Verdict (read first)

- **Go.** Demand is proven (7-Zip ecosystem pulls tens of thousands of GitHub stars and ~1.9M npm downloads/week for its binaries alone), and the two biggest incumbents monetize through annoyance (WinRAR nagware, Bandizip in-app ads).
- Nobody owns "one modern, open-source GUI + CLI, identical on macOS/Windows/Linux, with first-class zstd/brotli" — NanaZip is Windows-only, Keka is macOS-only, PeaZip is cross-platform but UI-cluttered, 7-Zip's GUI is Windows-only and dated.
- Do **not** compete on raw compression ratio (7z/LZMA2 wins) or on extraction-only basics (OSes now do that natively). Win on UX, format breadth, cross-platform parity, and zstd-speed workflows.

## 1. Competitor table

### GUI tools

| Tool | Platforms | Price / license | Formats (write / read) | Maintenance signal (Aug 2026) | Strengths | Notable complaints |
|---|---|---|---|---|---|---|
| **7-Zip** | Windows GUI; CLI ports (Linux/macOS `7zz`) | Free, LGPL (+unRAR restriction) ([7-zip.org](https://www.7-zip.org/download.html)) | 7z, zip, tar, gz, xz, zstd (since 24.01) write; rar, iso, etc. read ([nixpkgs #358040](https://github.com/NixOS/nixpkgs/issues/358040)) | Very active: 26.x releases in 2026 ([SourceForge files](https://sourceforge.net/projects/sevenzip/files/7-Zip/), [Guru3D](https://www.guru3d.com/download/7-zip-download/)) | Best ratio, tiny, trusted, scriptable | Dated Win32 UI; no auto-updater ([SourceForge reviews](https://sourceforge.net/projects/sevenzip/)); recurring RCE CVEs ([PCMag 2026-07](https://www.pcmag.com/news/free-archive-program-7-zip-can-be-hacked-with-a-malicious-file), [Tom's Hardware 2026-05](https://www.tomshardware.com/tech-industry/cyber-security/wide-ranging-7-zip-vulnerability-with-8-8-cve-rating-allows-for-code-execution-hundreds-of-millions-of-machines-potentially-at-risk)) |
| **WinRAR** | Windows; CLI for Linux/macOS | Trialware, ~$29 perpetual, 40-day trial ([EULA](https://www.win-rar.com/winrarlicense.html), [license FAQ](https://www.win-rar.com/license-perpetual-subscription.html?&l=4)) | RAR write (exclusive), zip; reads most | Active (7.10 added dark mode Nov 2024 — [MSPoweruser](https://mspoweruser.com/winrar-now-offers-dark-mode-support-you-can-now-download-its-beta-version/)) | Only tool that creates RAR; recovery records | Nagware purchase prompts ([XDA](https://www.xda-developers.com/reasons-to-use-7-zip-over-winrar/)); dark mode only arrived 2024 |
| **WinZip** | Windows, macOS, iOS/Android | Paid; suites $34.95–$89.95 ([TrustRadius](https://www.trustradius.com/products/winzip/pricing)); perpetual licensing being phased out ([Corel KB](https://kb.corel.com/en/125807)), subscription sold via retail ([Dell](https://www.dell.com/en-us/shop/download-winzip-30-standard-single-user/apd/ad514795/software)) | zip/zipx-centric; reads rar/7z | Active (v30) | Brand, enterprise features, cloud integration | Expensive for functionality OSes now ship free; "makes WinZip seem ancient and clunky" ([Alphr](https://www.alphr.com/password-protect-zip-macos/)) |
| **Keka** | macOS, iOS | Free direct download; paid on Mac App Store to fund dev ([keka.io](https://www.keka.io/en/)) | Writes 7z/zip/tar/gz/xz/**brotli/zstd**/wim…; reads rar & 30+ ([keka.io](https://www.keka.io/en/)) | Active (repo pushed 2026-06, 7.2k★ — [GitHub API](https://github.com/aonez/Keka)) | Native Mac UX, drag-to-dock, the de-facto Mac compressor | macOS-only; sandboxing quirks; weak Finder/context-menu integration (helper needed — [keka.io](https://www.keka.io/en/)) |
| **PeaZip** | Windows, Linux, macOS, BSD | Free, LGPLv3 ([GitHub](https://github.com/peazip/PeaZip)) | 200+ formats read; writes 7z/zip/tar/zstd/brotli/PEA… | Active (pushed 2026-07, 7.7k★ — [GitHub API](https://github.com/peazip/PeaZip)) | Closest existing rival to Squash's scope; encryption options | UI cluttered/dated; "not inputting/outputting consistently" ([Chaython note](https://github.com/Chaython/7-Zip-ZSTD-Portable)); "best on Linux" reputation |
| **Bandizip** | Windows, macOS | Free tier **ad-supported** since v7.0 (2020); Pro $30, Enterprise $20/PC ([gHacks](https://www.ghacks.net/2020/03/03/bandizip-7-0-introduces-commercial-versions-and-ads/), [edition comparison](https://en.bandisoft.com/bandizip/help/edition-comparison/)) | zip/7z/tar write; rar/iso read | Active (7.37, 2025) | Fast, pretty UI, image preview | Desktop popup ads in free 7.37 drove users to OSS alternatives ([appinn, 2025-02](https://www.appinn.com/bandizip-7-37/)) |
| **NanaZip** | Windows 10/11 only | Free, OSS ([GitHub](https://github.com/M2Team/NanaZip)) | 7-Zip core + Brotli, LZ4, **zstd**, Fast-LZMA2 ([nanazip.net](https://nanazip.net/)) | Very active (pushed 2026-08-06, 15.1k★, 370 open issues — [GitHub API](https://github.com/M2Team/NanaZip)); Microsoft Store App Awards winner ([zhihu summary](https://www.wdlinux.cn/html/ruanjian/20250204/79503.html)) | Modern Win11 UI (Mica/dark mode), context-menu integration, MSIX auto-update | Windows-only; inherits upstream 7-Zip CVEs with a lag ([SoftMemo on CVE-2025-0411](https://couleurapp.com/software/125)) |
| **The Unarchiver** | macOS | Free, proprietary (MacPaw) ([macpaw.com](https://macpaw.com/the-unarchiver)) | **Extract only** — rar, 7z, lzh, stuffit, legacy formats | Slow: v4.3.9, Mar 2025 ([theunarchiver.com](https://theunarchiver.com/)) | Zero-friction extraction, best obscure-format support on Mac | Cannot compress anything ([bundl.run](https://bundl.run/compare/keka-vs-the-unarchiver)); closed source |

### CLI / libraries (Squash's core will sit on these)

| Tool | Role | Signal | Notes |
|---|---|---|---|
| **gzip/zlib** | Ubiquitous baseline | Ships everywhere; single-threaded, mediocre ratio | Compatibility floor, not a differentiator |
| **xz / LZMA** | High-ratio CLI (Unix) | Active, but Mar 2024 supply-chain backdoor CVE-2024-3094 shook trust ([NVD](https://nvd.nist.gov/vuln/detail/CVE-2024-3094)) | Slow compression; ratio crown belongs to LZMA2 |
| **zstd** | Fast modern codec (Meta) | 27.5k★, pushed 2026-08 ([GitHub API](https://github.com/facebook/zstd)); decompressable natively by Win11 via libarchive | The speed/relevance axis Squash should build on; GUI support still rare outside Keka/PeaZip/NanaZip |
| **brotli** | Web-oriented codec (Google) | Standard in HTTP; GUI archiver support rare | Nice-to-have format checkbox |
| **libarchive / bsdtar** | Multi-format extraction engine | Powers Windows 11's native 7z/rar/tar extraction ([logicity](https://logicity.in/en/blog/windows-11-now-extracts-7z-and-rar-files-natively), [tweaknow](https://www.tweaknow.com/RegTweakNativeArchive.php)) and macOS `tar` | Both a dependency candidate and proof that OS vendors are eating basic extraction |

## 2. Demand signals

- **GitHub stars (fetched 2026-08-13 via API):** NanaZip 15,069 · PeaZip 7,741 · mcmilk/7-Zip-zstd 7,263 · Keka 7,205 · ip7z/7zip 3,706 · facebook/zstd 27,544. Combined 7-Zip-ecosystem ≈ 34k stars — a large, active OSS archiver audience.
- **Downloads:** `7zip-bin` npm package alone gets **~1.93M downloads/week** ([npmx](https://npmx.dev/search?q=keyword:7zip)); 7-Zip SourceForge rating 4.8/5 from 765 reviews ([SourceForge](https://sourceforge.net/projects/sevenzip/)).
- **Platform validation:** Microsoft built 7z/rar/tar extraction and 7z/tar creation into Windows 11 24H2 File Explorer ([pureinfotech](https://pureinfotech.com/windows-11-create-7-zip-tar-archive-files/)) — OS vendors don't native-ize dead categories.
- **Community discussion volume:** steady stream of 2024–2026 "which archiver / 7-Zip alternative" content: How-To Geek switching stories ([NanaZip](https://www.howtogeek.com/nanazip-7-zip-alternative/), [PeaZip](https://www.howtogeek.com/i-tried-replacing-7-zip-with-this-open-source-alternative-heres-how-it-went/)), XDA ([7-Zip vs WinRAR](https://www.xda-developers.com/reasons-to-use-7-zip-over-winrar/), [NanaZip hybrid](https://www.xda-developers.com/perfect-hybrid-of-7zip-and-winrar/)), gHacks Bandizip controversy threads ([2020](https://www.ghacks.net/2020/03/03/bandizip-7-0-introduces-commercial-versions-and-ads/), [2021](https://www.ghacks.net/2021/01/25/revisiting-bandizip-did-anything-change-since-version-7s-launch-controversy/)), active 2025 comparison threads in CN communities ([appinn](https://www.appinn.com/bandizip-7-37/), [Chiphell](https://www.chiphell.com/forum.php?mod=viewthread&tid=2670159)). Sustained "alternatives" journalism = sustained dissatisfaction.
- **Security-driven switching:** every 7-Zip CVE wave (Nov 2024 zstd RCE — [nixpkgs](https://github.com/NixOS/nixpkgs/issues/358040); May 2026 8.8 CVE — [Tom's Hardware](https://www.tomshardware.com/tech-industry/cyber-security/wide-ranging-7-zip-vulnerability-with-8-8-cve-rating-allows-for-code-execution-hundreds-of-millions-of-machines-potentially-at-risk); Jul 2026 XZ RCE — [The Hacker News](https://thehackernews.com/2026/07/new-7-zip-vulnerability-could-let.html)) triggers "what else should I use" threads; 7-Zip's lack of auto-update amplifies it ([SourceForge request](https://sourceforge.net/projects/sevenzip/)).

## 3. Complaint mining (recurring, sourced)

| Complaint | Incumbent(s) | Source |
|---|---|---|
| Dated, plain UI; dark mode arrived only in 2024 | 7-Zip, WinRAR | [TrishTech](https://www.trishtech.com/2022/10/nanazip-7-zip-fork-with-modern-user-interface/), [MSPoweruser](https://mspoweruser.com/winrar-now-offers-dark-mode-support-you-can-now-download-its-beta-version/), [appinn (UI "丑/technical")](https://www.appinn.com/bandizip-7-37/) |
| No auto-updater + repeated RCE CVEs = patch anxiety | 7-Zip | [SourceForge reviews/requests](https://sourceforge.net/projects/sevenzip/), [PCMag](https://www.pcmag.com/news/free-archive-program-7-zip-can-be-hacked-with-a-malicious-file) |
| Nagware purchase prompts after 40-day trial | WinRAR | [XDA](https://www.xda-developers.com/reasons-to-use-7-zip-over-winrar/) |
| In-app and desktop popup ads in free tier | Bandizip | [gHacks](https://www.ghacks.net/2021/01/25/revisiting-bandizip-did-anything-change-since-version-7s-launch-controversy/), [appinn](https://www.appinn.com/bandizip-7-37/) |
| Paid software for what feels like a solved problem | WinZip ($34.95+), WinRAR | [TrustRadius](https://www.trustradius.com/products/winzip/pricing), [Alphr](https://www.alphr.com/password-protect-zip-macos/) |
| macOS has no single great compress+extract app (Keka compresses, Unarchiver extracts) | Mac ecosystem | [bundl.run](https://bundl.run/compare/keka-vs-the-unarchiver), [keka.io](https://www.keka.io/en/) |
| No modern-codec (zstd/brotli) support in 7-Zip GUI for years → fork ecosystem | 7-Zip | [mcmilk/7-Zip-zstd (7.3k★)](https://github.com/mcmilk/7-Zip-zstd), [xitongzhijia](https://www.xitongzhijia.net/news/20240320/297697.html) |
| Windows 11 context-menu breakage ("show more options") for 7-Zip | 7-Zip on Win11 | [BounceGeek](https://bouncegeek.com/add-winrar-and-7zip-on-windows-11-context-menu/), [How-To Geek](https://www.howtogeek.com/nanazip-7-zip-alternative/) |
| Cluttered UI, inconsistent behavior | PeaZip | [Chaython/7-Zip-ZSTD-Portable](https://github.com/Chaython/7-Zip-ZSTD-Portable), [How-To Geek](https://www.howtogeek.com/i-tried-replacing-7-zip-with-this-open-source-alternative-heres-how-it-went/) |
| Supply-chain trust shock (backdoor) | xz | [NVD CVE-2024-3094](https://nvd.nist.gov/vuln/detail/CVE-2024-3094) |

## 4. Gap analysis — where Squash can and can't win

**Can credibly win:**
- **Cross-platform parity:** no tool offers one modern GUI + scriptable CLI on all three desktop OSes. NanaZip = Windows-only; Keka/Unarchiver = macOS-only; 7-Zip GUI = Windows-only; PeaZip is the only attempt and loses on UI polish (all sourced above).
- **Modern UX as default:** dark mode, native context menus/Finder integration, drag-and-drop presets — complaints above show incumbents bolt these on late or never.
- **zstd-first workflows:** fast preset ("send now" = zstd level 3, "archive forever" = 7z/xz) with GUI support that most incumbents lacked until recently.
- **Trust bundle:** open source + signed/notarized binaries + opt-in update checks + no ads/nags directly answers every complaint in §3 (ads, nagware, no auto-update, CVE patch anxiety). Roadmap's Phase 3 security audit (zip-slip, bombs) is a marketable differentiator.
- **macOS "one app" gap:** compress *and* extract in a single native-feeling app (Keka + Unarchiver in one).

**Can't win (be honest):**
- **Raw ratio:** 7z/LZMA2 remains the ratio king; claiming "smaller files than 7-Zip" is not credible. Compete on speed-per-byte via zstd instead.
- **RAR creation:** proprietary; only WinRAR writes RAR. Extraction-only forever.
- **Basic extraction on Windows:** Win11 24H2+ does 7z/rar/tar natively ([tweaknow](https://www.tweaknow.com/RegTweakNativeArchive.php)) — casual extraction users are gone; the market is power users, creators, and cross-platform teams.
- **Brand/trust overnight:** 7-Zip's 25-year reputation and enterprise entrenchment won't be displaced; aim to be the *recommended modern default*, not the 7-Zip killer.

## 5. Keywords / discoverability (for README, site, store listings)

Seed terms (from user language in the sourced threads/articles; no reliable volume tool was available in this environment — validate volumes with Google Trends/Keyword Planner before final SEO):

- Navigational/high-intent: `7-zip alternative`, `winrar alternative free`, `7-zip alternative mac`, `7-zip alternative windows 11`, `keka for windows`
- Task-based: `open rar mac`, `open 7z mac`, `extract rar windows 11`, `compress files windows 11`, `how to zip files on mac`, `create tar.gz windows`, `open zstd file`
- Problem-based: `bandizip ads alternative`, `winrar trial expired`, `7-zip dark mode`, `7-zip update`, `file archiver open source`
- Codec-curious: `zstd gui`, `zstandard compressor windows`, `brotli archive tool`
- Long tail: `best free file archiver 2026`, `cross platform file compressor`, `open source winrar alternative`

Placement follows the skill split: this doc finds the terms; `app-store-optimization`-style placement (repo description, README H1/H2, package descriptions for brew/winget) happens in Phase 6 README polish.

## 6. Verdict (3 lines)

**Go.** The market is huge, proven, and its leaders are either ad/nag-monetized, platform-locked, or UI-frozen — while OSS archiver projects demonstrably attract tens of thousands of stars and millions of downloads.
Squash's defensible niche is the intersection nobody occupies: one open-source, modern-UI, zstd-first **GUI + CLI identical on macOS/Windows/Linux** with signed builds and a security-audited core.
Primary risks: PeaZip modernizes first; Windows/macOS native tooling keeps absorbing casual users; and ratio-focused users are unreachable — so benchmark honestly and lead with UX, speed, and trust.
