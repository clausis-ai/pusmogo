/// What choosing a microphone needs to know about one input.
public struct MicOption {
    public let id: UInt32
    public let bluetooth: Bool
    /// Preference in place of a Bluetooth default: 0 built-in, 1 wired, nil for
    /// inputs never picked automatically (virtual, phone, Bluetooth).
    public let wiredRank: Int?

    public init(id: UInt32, bluetooth: Bool = false, wiredRank: Int? = nil) {
        self.id = id
        self.bluetooth = bluetooth
        self.wiredRank = wiredRank
    }
}

/// The input to record from when the user picked none: the system default,
/// but a built-in or wired microphone in place of a Bluetooth one, because an
/// open Bluetooth microphone turns the headset's music into a phone call. A
/// closed MacBook's own microphone is switched off in hardware, so it is skipped.
public func automaticMic(_ options: [MicOption], systemDefault: UInt32?, lidClosed: Bool) -> UInt32? {
    let system = options.first { $0.id == systemDefault }
    if let system, !system.bluetooth { return system.id }
    let wired = options.filter { o in o.wiredRank.map { !($0 == 0 && lidClosed) } ?? false }
    return (wired.min { ($0.wiredRank ?? .max) < ($1.wiredRank ?? .max) } ?? system)?.id
}
