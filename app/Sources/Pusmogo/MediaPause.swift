import CoreAudio
import Darwin
import Foundation

/// Pauses music or video that is playing while the user dictates, and resumes
/// it afterwards.
///
/// macOS no longer tells ordinary apps whether the "Now Playing" app is
/// playing, so the check uses public Core Audio instead: which processes are
/// sending sound to an output right now. Only when some other process is
/// playing does it send the system "pause" command (the one the keyboard's
/// media key sends). It then looks again: resume is sent only if one of those
/// processes actually went quiet, so a paused player is never started and
/// sound that ignores the command (a call, a game) is left alone.
@MainActor
enum MediaPause {
    /// `PUSMOGO_DEBUG_MEDIA=1` logs each decision (tests).
    private static let debug = ProcessInfo.processInfo.environment["PUSMOGO_DEBUG_MEDIA"] == "1"

    private enum State: Equatable {
        case idle
        /// Pause sent; waiting to see whether the players went quiet.
        case checking(endedEarly: Bool)
        case paused
    }

    private static var state = State.idle

    /// Call when a real dictation starts.
    static func begin() {
        guard state == .idle else { return }
        let playing = processesPlaying()
        guard !playing.isEmpty else { log("nothing playing"); return }
        guard send(.pause) else { log("pause command unavailable"); return }
        state = .checking(endedEarly: false)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
            let quiet = !playing.subtracting(processesPlaying()).isEmpty
            log("pause sent to \(playing.count) playing; went quiet: \(quiet)")
            if case .checking(let endedEarly) = state {
                if !quiet {
                    state = .idle
                } else if endedEarly {
                    state = .idle
                    resume()
                } else {
                    state = .paused
                }
            }
        }
    }

    /// Call when the recording ends (release, cancel or discard).
    static func end() {
        switch state {
        case .idle: break
        case .checking: state = .checking(endedEarly: true)
        case .paused:
            state = .idle
            resume()
        }
    }

    private static func resume() {
        log("resume")
        _ = send(.play)
    }

    private static func log(_ s: String) {
        if debug { NSLog("media: %@", s) }
    }

    // MARK: - Core Audio: who is playing

    /// PIDs of other processes currently sending audio to an output device.
    static func processesPlaying() -> Set<pid_t> {
        let system = AudioObjectID(kAudioObjectSystemObject)
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyProcessObjectList,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr, size > 0 else { return [] }
        var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &ids) == noErr else { return [] }
        let me = getpid()
        var result = Set<pid_t>()
        for id in ids {
            var running: UInt32 = 0
            var pid: pid_t = 0
            guard read(id, kAudioProcessPropertyIsRunningOutput, &running), running != 0,
                  read(id, kAudioProcessPropertyPID, &pid), pid != me
            else { continue }
            result.insert(pid)
        }
        return result
    }

    private static func read<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ value: inout T) -> Bool {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size = UInt32(MemoryLayout<T>.size)
        return AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value) == noErr
    }

    // MARK: - Media commands

    private enum Command: UInt32 {
        case play = 0
        case pause = 1
    }

    private typealias SendCommand = @convention(c) (UInt32, CFDictionary?) -> Bool

    /// The system media command sender, looked up at run time (not public API;
    /// nil if a future macOS drops it, and media pause then does nothing).
    private static let sendCommand: SendCommand? = {
        guard let handle = dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_LAZY),
              let symbol = dlsym(handle, "MRMediaRemoteSendCommand")
        else { return nil }
        return unsafeBitCast(symbol, to: SendCommand.self)
    }()

    private static func send(_ command: Command) -> Bool {
        sendCommand?(command.rawValue, nil) ?? false
    }
}
