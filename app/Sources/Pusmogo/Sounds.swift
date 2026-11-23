import AVFoundation

/// Pusmogo's own UI sounds, synthesised at launch (no audio assets, nothing
/// borrowed). Short, soft sine "pops" with a fast attack and smooth decay.
@MainActor
final class Sounds {
    enum Cue { case start, stop, lock, cancel, error, swap }

    var enabled = true
    private var players: [Cue: AVAudioPlayer] = [:]

    init() {
        players[.start] = Self.player([(740, 0.055), (988, 0.075)], volume: 0.22)
        players[.stop] = Self.player([(988, 0.05), (740, 0.07)], volume: 0.18)
        players[.lock] = Self.player([(1175, 0.035), (0, 0.03), (1175, 0.045)], volume: 0.2)
        players[.cancel] = Self.player([(523, 0.07)], volume: 0.16)
        players[.swap] = Self.player([(880, 0.04), (1320, 0.06)], volume: 0.16)
        players[.error] = Self.player([(392, 0.09), (330, 0.11)], volume: 0.2)
    }

    func play(_ cue: Cue) {
        guard enabled, let p = players[cue] else { return }
        p.currentTime = 0
        p.play()
    }

    /// Notes are (frequency Hz, seconds); frequency 0 is a rest.
    private static func player(_ notes: [(Double, Double)], volume: Float) -> AVAudioPlayer? {
        let rate = 44_100.0
        var samples: [Int16] = []
        for (freq, dur) in notes {
            let n = Int(rate * dur)
            for i in 0..<n {
                let t = Double(i) / rate
                let attack = min(1, t / 0.006)
                let decay = exp(-5.5 * t / dur)
                let v = freq == 0 ? 0 : sin(2 * .pi * freq * t) * attack * decay
                // A touch of the octave makes it rounder, less "beep".
                let o = freq == 0 ? 0 : 0.18 * sin(4 * .pi * freq * t) * attack * decay
                samples.append(Int16(max(-1, min(1, (v + o) * 0.8)) * Double(Int16.max)))
            }
        }
        let player = try? AVAudioPlayer(data: wav(samples, rate: Int(rate)))
        player?.volume = volume
        player?.prepareToPlay()
        return player
    }

    private static func wav(_ samples: [Int16], rate: Int) -> Data {
        var d = Data()
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        func u16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        let dataSize = UInt32(samples.count * 2)
        d.append(contentsOf: Array("RIFF".utf8)); u32(36 + dataSize)
        d.append(contentsOf: Array("WAVE".utf8))
        d.append(contentsOf: Array("fmt ".utf8)); u32(16); u16(1); u16(1)
        u32(UInt32(rate)); u32(UInt32(rate * 2)); u16(2); u16(16)
        d.append(contentsOf: Array("data".utf8)); u32(dataSize)
        samples.withUnsafeBytes { d.append(contentsOf: $0) }
        return d
    }
}
