// swift-tools-version:5.10
import PackageDescription

// scripts/build.sh builds the Rust static library first and stages it in core/target/lib
// (host architecture, or arm64 + x86_64 for a universal build).
let package = Package(
    name: "Pusmogo",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CPusmogoCore", path: "Sources/CPusmogoCore"),
        // Pure logic with no system APIs, so `swift test` can run it.
        .target(name: "PusmogoKeys", path: "Sources/PusmogoKeys"),
        .executableTarget(
            name: "Pusmogo",
            dependencies: ["CPusmogoCore", "PusmogoKeys", "Sparkle"],
            path: "Sources/Pusmogo",
            linkerSettings: [
                .unsafeFlags(["-L../core/target/lib"]),
                // scripts/build.sh copies Sparkle.framework into the bundle.
                .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"]),
                .linkedFramework("Metal"),
                .linkedFramework("MetalKit"),
                .linkedFramework("Accelerate"),
                .linkedFramework("Foundation"),
                .linkedLibrary("c++"),
                .linkedLibrary("sqlite3"),
            ]
        ),
        // App updates (MIT). SwiftPM checks the hash, the one in Sparkle's own manifest.
        .binaryTarget(
            name: "Sparkle",
            url: "https://github.com/sparkle-project/Sparkle/releases/download/2.10.1/Sparkle-for-Swift-Package-Manager.zip",
            checksum: "4c2e48a3e50ebcc2c88a58a4c7264a1a99c3c31857ce754357afb9cdb323022a"
        ),
        .testTarget(
            name: "PusmogoKeysTests",
            dependencies: ["PusmogoKeys"],
            path: "Tests/PusmogoKeysTests"
        ),
    ]
)
