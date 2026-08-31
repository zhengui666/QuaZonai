# Release Build Settings

The Universal target is archived with Swift 6, an iOS/iPadOS 18 minimum deployment target, device family `1,2`, hardened runtime defaults, and signing disabled only in CI. Distribution signing, provisioning, App Store Connect upload, and review credentials remain deployment-time concerns and are not committed to the repository.

CI must compile the same shared scheme for an iPhone simulator, an iPad simulator, and `generic/platform=iOS`. It must retain `.xcresult`, build logs, and the unsigned `.xcarchive` when a failure occurs. Release builds may not define certificate-trust bypasses, embed machine credentials, enable arbitrary HTTP transport, or persist TOTP/provider secrets.
