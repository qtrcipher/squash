# Homebrew cask template for the Squash GUI (macOS .dmg per arch).
#
# Rendered by packaging/homebrew/publish.sh during the release workflow:
# every @TOKEN@ is replaced with the real version / SHA256 from the release's
# SHA256SUMS.txt, then pushed to Casks/squash.rb in qtrcipher/homebrew-tap.
# Do not edit the tap repo by hand — edit this template.
cask "squash" do
  arch arm: "aarch64", intel: "x64"

  version "@VERSION@"
  sha256 arm:   "@SHA256_DMG_AARCH64@",
         intel: "@SHA256_DMG_X64@"

  url "https://github.com/qtrcipher/squash/releases/download/v#{version}/Squash_#{version}_#{arch}.dmg"
  name "Squash"
  desc "Open-source file compressor for macOS, Windows, and Linux"
  homepage "https://github.com/qtrcipher/squash"

  app "Squash.app"

  zap trash: [
    "~/Library/Application Support/dev.squash.app",
    "~/Library/Caches/dev.squash.app",
    "~/Library/Preferences/dev.squash.app.plist",
  ]
end
