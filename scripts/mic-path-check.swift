// Checks the capture conversion for signal damage, offline.
//
// Usage: swift scripts/mic-path-check.swift <16 kHz mono WAV> [rate] [channels] [chunk] [out.wav]
//
// Upsamples the clip to the given device format (default 48000 Hz, 1 channel),
// cuts it into tap-sized buffers (default 4800 frames) and converts them back
// to 16 kHz mono the way `AudioRecorder.append` does: one AVAudioConverter for
// the whole recording, one input buffer per call. Prints the length change,
// the lag that best aligns output and source, and the signal-to-error ratio
// after alignment, overall and for the first 300 ms. Keep `convertLikeApp` in
// step with `AudioRecorder.append`. With an output path, the converted audio
// is written there (16 kHz mono 16-bit) so it can be transcribed with the bench.
// `--no-drain` (anywhere) skips the end-of-stream drain that stop() does.

import AVFoundation

func fail(_ s: String) -> Never { FileHandle.standardError.write((s + "\n").data(using: .utf8)!); exit(1) }

let drain = !CommandLine.arguments.contains("--no-drain")
let args = CommandLine.arguments.filter { $0 != "--no-drain" }
guard args.count >= 2 else { fail("usage: mic-path-check.swift <wav> [rate] [channels] [chunk]") }
let rate = args.count > 2 ? Double(args[2])! : 48_000
let channels = args.count > 3 ? AVAudioChannelCount(args[3])! : 1
let chunk = args.count > 4 ? AVAudioFrameCount(args[4])! : 4800

let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
let file = try AVAudioFile(forReading: URL(fileURLWithPath: args[1]))
guard file.processingFormat.sampleRate == 16_000, file.processingFormat.channelCount == 1 else { fail("needs 16 kHz mono") }
let src = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length))!
try file.read(into: src)
let source = Array(UnsafeBufferPointer(start: src.floatChannelData![0], count: Int(src.frameLength)))

/// The device-side signal: one-shot, highest quality, so it is not the thing under test.
func toDevice(_ x: [Float]) -> AVAudioPCMBuffer {
    let dev = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: rate, channels: channels, interleaved: false)!
    let conv = AVAudioConverter(from: target, to: dev)!
    conv.sampleRateConverterQuality = AVAudioQuality.max.rawValue
    conv.sampleRateConverterAlgorithm = AVSampleRateConverterAlgorithm_Mastering
    let inBuf = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: AVAudioFrameCount(x.count))!
    inBuf.frameLength = inBuf.frameCapacity
    x.withUnsafeBufferPointer { inBuf.floatChannelData![0].update(from: $0.baseAddress!, count: x.count) }
    let out = AVAudioPCMBuffer(pcmFormat: dev, frameCapacity: AVAudioFrameCount(Double(x.count) * rate / 16_000) + 4096)!
    var fed = false
    var err: NSError?
    conv.convert(to: out, error: &err) { _, st in
        if fed { st.pointee = .endOfStream; return nil }
        fed = true; st.pointee = .haveData; return inBuf
    }
    if let err { fail("upsample: \(err)") }
    return out
}

/// Same calls as `AudioRecorder.append`, per tap buffer.
func convertLikeApp(_ pcm: AVAudioPCMBuffer, _ converter: AVAudioConverter, into buffer: inout [Float]) {
    let ratio = target.sampleRate / pcm.format.sampleRate
    let capacity = AVAudioFrameCount(Double(pcm.frameLength) * ratio) + 32
    guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return }
    var fed = false
    var error: NSError?
    converter.convert(to: out, error: &error) { _, status in
        if fed { status.pointee = .noDataNow; return nil }
        fed = true
        status.pointee = .haveData
        return pcm
    }
    guard error == nil, let ch = out.floatChannelData?[0] else { return }
    buffer.append(contentsOf: UnsafeBufferPointer(start: ch, count: Int(out.frameLength)))
}

let device = toDevice(source)

/// What a careful one-shot conversion back to 16 kHz gives: the best the
/// device signal allows, so band-edge loss of the round trip is not blamed
/// on the app.
func ideal(_ dev: AVAudioPCMBuffer) -> [Float] {
    let conv = AVAudioConverter(from: dev.format, to: target)!
    conv.sampleRateConverterQuality = AVAudioQuality.max.rawValue
    conv.sampleRateConverterAlgorithm = AVSampleRateConverterAlgorithm_Mastering
    let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: AVAudioFrameCount(source.count) + 4096)!
    var fed = false
    var err: NSError?
    conv.convert(to: out, error: &err) { _, st in
        if fed { st.pointee = .endOfStream; return nil }
        fed = true; st.pointee = .haveData; return dev
    }
    return Array(UnsafeBufferPointer(start: out.floatChannelData![0], count: Int(out.frameLength)))
}
let reference = ideal(device)
let converter = AVAudioConverter(from: device.format, to: target)!
var got: [Float] = []
var at: AVAudioFrameCount = 0
while at < device.frameLength {
    let n = min(chunk, device.frameLength - at)
    let piece = AVAudioPCMBuffer(pcmFormat: device.format, frameCapacity: n)!
    piece.frameLength = n
    for c in 0..<Int(channels) {
        piece.floatChannelData![c].update(from: device.floatChannelData![c] + Int(at), count: Int(n))
    }
    convertLikeApp(piece, converter, into: &got)
    at += n
}
if drain {
    // Same as `AudioRecorder.drainTail` at stop().
    let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: 4096)!
    var err: NSError?
    converter.convert(to: out, error: &err) { _, st in st.pointee = .endOfStream; return nil }
    if err == nil { got.append(contentsOf: UnsafeBufferPointer(start: out.floatChannelData![0], count: Int(out.frameLength))) }
}

func rms(_ x: ArraySlice<Float>) -> Double {
    x.isEmpty ? 0 : (x.reduce(0.0) { $0 + Double($1 * $1) } / Double(x.count)).squareRoot()
}

/// Best lag of `got` against `source` in -64...64 samples, by correlation.
var bestLag = 0, bestCorr = -Double.infinity
for lag in -64...64 {
    var c = 0.0
    for i in 0..<min(source.count, got.count) where i + lag >= 0 && i + lag < got.count { c += Double(source[i] * got[i + lag]) }
    if c > bestCorr { bestCorr = c; bestLag = lag }
}
func ser(_ range: Range<Int>, against ref: [Float] = source) -> Double {
    var sig = 0.0, err = 0.0
    for i in range where i < ref.count && i + bestLag >= 0 && i + bestLag < got.count {
        sig += Double(ref[i] * ref[i])
        let e = Double(ref[i] - got[i + bestLag]); err += e * e
    }
    return 10 * log10(sig / max(err, 1e-20))
}
let n = min(source.count, got.count)
let head = min(n, 4800)
let peak = got.map { abs($0) }.max() ?? 0
print(String(format: "device %.0f Hz x%d, chunk %d: source %d samples, got %d (%+d, %.1f ms)",
             rate, channels, chunk, source.count, got.count, got.count - source.count,
             Double(got.count - source.count) / 16))
print(String(format: "lag %d samples (%.2f ms); signal/error %.1f dB overall, %.1f dB in the first 300 ms; rms %.4f -> %.4f; peak %.3f",
             bestLag, Double(bestLag) / 16, ser(0..<n), ser(0..<head),
             rms(source[0..<n]), rms(got[0..<n]), peak))
print(String(format: "against a one-shot max-quality conversion of the same device signal: %.1f dB overall, %.1f dB in the first 300 ms",
             ser(0..<n, against: reference), ser(0..<head, against: reference)))

if args.count > 5 {
    let fmt = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16_000, channels: 1, interleaved: false)!
    let outFile = try AVAudioFile(forWriting: URL(fileURLWithPath: args[5]), settings: fmt.settings,
                                  commonFormat: .pcmFormatFloat32, interleaved: false)
    let buf = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: AVAudioFrameCount(got.count))!
    buf.frameLength = buf.frameCapacity
    got.withUnsafeBufferPointer { buf.floatChannelData![0].update(from: $0.baseAddress!, count: got.count) }
    try outFile.write(from: buf)
}
