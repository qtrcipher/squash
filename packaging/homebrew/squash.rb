# Homebrew formula template for the Squash CLI.
#
# Rendered by packaging/homebrew/publish.sh during the release workflow:
# every @TOKEN@ is replaced with the real version / SHA256 from the release's
# SHA256SUMS.txt, then pushed to Formula/squash.rb in qtrcipher/homebrew-tap.
# Do not edit the tap repo by hand — edit this template.
class Squash < Formula
  desc "Open-source file compressor — CLI (GUI available as cask qtrcipher/tap/squash)"
  homepage "https://github.com/qtrcipher/squash"
  version "@VERSION@"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/qtrcipher/squash/releases/download/v@VERSION@/squash-macos-aarch64.tar.gz"
      sha256 "@SHA256_MACOS_AARCH64@"
    end
    on_intel do
      url "https://github.com/qtrcipher/squash/releases/download/v@VERSION@/squash-macos-x86_64.tar.gz"
      sha256 "@SHA256_MACOS_X86_64@"
    end
  end

  on_linux do
    url "https://github.com/qtrcipher/squash/releases/download/v@VERSION@/squash-linux-x86_64.tar.gz"
    sha256 "@SHA256_LINUX_X86_64@"
  end

  def install
    bin.install "squash"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/squash --version")
  end
end
