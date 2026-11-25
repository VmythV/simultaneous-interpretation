// audio-tap — 抓取 macOS 系统正在播放的声音（Core Audio Process Tap，需要 macOS 14.2+）
//
// 用法：
//   audio-tap [--watch-stdin]   开始采集：stdout 持续输出 f32le 单声道 PCM（原生采样率，见 format 事件）
//   audio-tap --meter           调试用：不输出 PCM，每 0.5 秒在 stderr 打印一次音量
//   audio-tap --check           只检查"系统录音"权限，打印 JSON 后退出
//   audio-tap --request         请求"系统录音"权限（弹系统授权框），打印 JSON 后退出
//   audio-tap --mic-check       检查麦克风权限
//   audio-tap --mic-request     请求麦克风权限
//   （权限归属于启动本程序的 App）
//
// stderr：每行一个 JSON 事件，例如
//   {"event":"format","sampleRate":48000,"channels":1}
//   {"event":"started","permission":"granted"}
//   {"event":"level","rms":0.012,"peak":0.2}
//   {"event":"error","stage":"create-tap","code":-1,"message":"..."}
//
// 退出条件：SIGINT / SIGTERM、stdout 管道断开、--watch-stdin 时 stdin 被关闭（父进程退出）。

import AVFoundation
import AudioToolbox
import CoreAudio
import Foundation

setvbuf(stdout, nil, _IONBF, 0)
signal(SIGPIPE, SIG_IGN)

func emit(_ dict: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: dict, options: [.sortedKeys]) else { return }
    FileHandle.standardError.write(data)
    FileHandle.standardError.write("\n".data(using: .utf8)!)
}

func fail(_ stage: String, _ status: OSStatus, _ message: String) -> Never {
    emit(["event": "error", "stage": stage, "code": Int(status), "message": message])
    exit(1)
}

// MARK: - 权限（TCC 私有接口，仅用于自用场景的状态检测）

enum Permission {
    typealias PreflightFn = @convention(c) (CFString, CFDictionary?) -> Int
    typealias RequestFn = @convention(c) (CFString, CFDictionary?, @escaping @convention(block) (Bool) -> Void) -> Void

    static let service = "kTCCServiceAudioCapture" as CFString
    static let handle = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW)

    /// 返回 (状态, 原始值)。状态：granted / denied / undetermined / unknown
    static func preflight() -> (String, Int) {
        guard let h = handle, let sym = dlsym(h, "TCCAccessPreflight") else { return ("unknown", -1) }
        let raw = unsafeBitCast(sym, to: PreflightFn.self)(service, nil)
        switch raw {
        case 0: return ("granted", raw)
        case 1: return ("denied", raw)
        default: return ("undetermined", raw)
        }
    }

    static func request() -> Bool? {
        guard let h = handle, let sym = dlsym(h, "TCCAccessRequest") else { return nil }
        let sem = DispatchSemaphore(value: 0)
        var granted = false
        unsafeBitCast(sym, to: RequestFn.self)(service, nil) { ok in
            granted = ok
            sem.signal()
        }
        sem.wait()
        return granted
    }
}

let args = Set(CommandLine.arguments.dropFirst())

if args.contains("--check") {
    let (state, raw) = Permission.preflight()
    emit(["event": "permission", "permission": state, "raw": raw])
    exit(0)
}

if args.contains("--request") {
    let granted = Permission.request()
    let (state, raw) = Permission.preflight()
    emit(["event": "permission", "requestResult": granted.map { $0 ? "granted" : "denied" } ?? "unavailable",
          "permission": state, "raw": raw])
    exit(0)
}

func micState() -> String {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: return "granted"
    case .denied, .restricted: return "denied"
    case .notDetermined: return "undetermined"
    @unknown default: return "unknown"
    }
}

if args.contains("--mic-check") {
    emit(["event": "permission", "permission": micState()])
    exit(0)
}

if args.contains("--mic-request") {
    let sem = DispatchSemaphore(value: 0)
    AVCaptureDevice.requestAccess(for: .audio) { _ in sem.signal() }
    sem.wait()
    emit(["event": "permission", "permission": micState()])
    exit(0)
}

let meterMode = args.contains("--meter")

// MARK: - Core Audio 工具函数

func defaultOutputDeviceUID() -> String {
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioHardwarePropertyDefaultOutputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain)
    var device = AudioDeviceID(kAudioObjectUnknown)
    var size = UInt32(MemoryLayout<AudioDeviceID>.size)
    var st = AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &device)
    guard st == noErr, device != kAudioObjectUnknown else { fail("default-output", st, "无法获取默认输出设备") }

    addr.mSelector = kAudioDevicePropertyDeviceUID
    var uid: Unmanaged<CFString>?
    size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    st = AudioObjectGetPropertyData(device, &addr, 0, nil, &size, &uid)
    guard st == noErr, let value = uid?.takeRetainedValue() else { fail("output-uid", st, "无法获取输出设备 UID") }
    return value as String
}

// MARK: - 创建 Tap + 聚合设备

let tapDescription = CATapDescription(monoGlobalTapButExcludeProcesses: [])
tapDescription.uuid = UUID()
tapDescription.name = "TingYi System Audio"
tapDescription.isPrivate = true
tapDescription.muteBehavior = .unmuted

var tapID = AudioObjectID(kAudioObjectUnknown)
var status = AudioHardwareCreateProcessTap(tapDescription, &tapID)
guard status == noErr else { fail("create-tap", status, "创建系统声音 Tap 失败") }

var formatAddr = AudioObjectPropertyAddress(
    mSelector: kAudioTapPropertyFormat,
    mScope: kAudioObjectPropertyScopeGlobal,
    mElement: kAudioObjectPropertyElementMain)
var asbd = AudioStreamBasicDescription()
var asbdSize = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
status = AudioObjectGetPropertyData(tapID, &formatAddr, 0, nil, &asbdSize, &asbd)
guard status == noErr else { fail("tap-format", status, "读取 Tap 音频格式失败") }
guard asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0, asbd.mBitsPerChannel == 32 else {
    fail("tap-format", -1, "不支持的 Tap 格式：flags=\(asbd.mFormatFlags) bits=\(asbd.mBitsPerChannel)")
}

let outputUID = defaultOutputDeviceUID()
let aggregateDescription: [String: Any] = [
    kAudioAggregateDeviceNameKey: "TingYi-Tap",
    kAudioAggregateDeviceUIDKey: UUID().uuidString,
    kAudioAggregateDeviceMainSubDeviceKey: outputUID,
    kAudioAggregateDeviceIsPrivateKey: true,
    kAudioAggregateDeviceIsStackedKey: false,
    kAudioAggregateDeviceTapAutoStartKey: true,
    kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID]],
    kAudioAggregateDeviceTapListKey: [[
        kAudioSubTapDriftCompensationKey: true,
        kAudioSubTapUIDKey: tapDescription.uuid.uuidString,
    ]],
]

var aggregateID = AudioObjectID(kAudioObjectUnknown)
status = AudioHardwareCreateAggregateDevice(aggregateDescription as CFDictionary, &aggregateID)
guard status == noErr else {
    AudioHardwareDestroyProcessTap(tapID)
    fail("create-aggregate", status, "创建聚合设备失败")
}

emit(["event": "format", "sampleRate": Int(asbd.mSampleRate), "channels": 1])

// MARK: - 采集回调

var procID: AudioDeviceIOProcID?
var mono = [Float](repeating: 0, count: 4096)
let sampleRate = asbd.mSampleRate
// 没有任何声音在播放时，系统不会回调；用补零保持输出是连续的时间轴，
// 否则下游 VAD 等不到"静音"，最后一句永远无法断句。
var lastWrite = DispatchTime.now()
let fillAfterSeconds = 0.15
var zeros = [Float](repeating: 0, count: Int(sampleRate))
var meterSumSquares: Double = 0
var meterPeak: Float = 0
var meterCount = 0
let meterWindow = Int(asbd.mSampleRate / 2)
var stopping = false

func shutdown(_ code: Int32) -> Never {
    if let p = procID {
        AudioDeviceStop(aggregateID, p)
        AudioDeviceDestroyIOProcID(aggregateID, p)
    }
    AudioHardwareDestroyAggregateDevice(aggregateID)
    AudioHardwareDestroyProcessTap(tapID)
    exit(code)
}

let ioQueue = DispatchQueue(label: "app.tingyi.audio-tap", qos: .userInteractive)
status = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, ioQueue) { _, inInputData, _, _, _ in
    if stopping { return }
    let buffers = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inInputData))
    guard let first = buffers.first, first.mNumberChannels > 0 else { return }
    let frames = Int(first.mDataByteSize) / (4 * Int(first.mNumberChannels))
    if frames == 0 { return }
    if mono.count < frames { mono = [Float](repeating: 0, count: frames) }

    // 所有缓冲区、所有声道混成单声道
    var totalChannels = 0
    for i in 0..<frames { mono[i] = 0 }
    for buffer in buffers {
        guard let data = buffer.mData else { continue }
        let nch = Int(buffer.mNumberChannels)
        let n = min(frames, Int(buffer.mDataByteSize) / (4 * nch))
        let samples = data.assumingMemoryBound(to: Float.self)
        for i in 0..<n {
            var sum: Float = 0
            for c in 0..<nch { sum += samples[i * nch + c] }
            mono[i] += sum
        }
        totalChannels += nch
    }
    if totalChannels > 1 {
        let scale = 1 / Float(totalChannels)
        for i in 0..<frames { mono[i] *= scale }
    }

    if meterMode {
        for i in 0..<frames {
            let s = mono[i]
            meterSumSquares += Double(s * s)
            meterPeak = max(meterPeak, abs(s))
        }
        meterCount += frames
        if meterCount >= meterWindow {
            let rms = (meterSumSquares / Double(meterCount)).squareRoot()
            emit(["event": "level", "rms": (rms * 10000).rounded() / 10000, "peak": (Double(meterPeak) * 10000).rounded() / 10000])
            meterSumSquares = 0; meterPeak = 0; meterCount = 0
        }
        return
    }

    lastWrite = DispatchTime.now()
    let written = mono.withUnsafeBufferPointer { fwrite($0.baseAddress, 4, frames, stdout) }
    if written < frames {
        stopping = true
        DispatchQueue.main.async { shutdown(0) }  // 读取方已关闭管道
    }
}
guard status == noErr else { fail("create-ioproc", status, "创建采集回调失败") }

// 静音补齐：与回调在同一个串行队列上，避免并发写 stdout
let filler = DispatchSource.makeTimerSource(queue: ioQueue)
filler.schedule(deadline: .now() + 0.05, repeating: 0.05)
filler.setEventHandler {
    if stopping || meterMode { return }
    let now = DispatchTime.now()
    let gap = Double(now.uptimeNanoseconds - lastWrite.uptimeNanoseconds) / 1e9
    guard gap >= fillAfterSeconds else { return }
    var remaining = Int(gap * sampleRate)
    lastWrite = now
    while remaining > 0 {
        let n = min(remaining, zeros.count)
        let written = zeros.withUnsafeBufferPointer { fwrite($0.baseAddress, 4, n, stdout) }
        if written < n {
            stopping = true
            DispatchQueue.main.async { shutdown(0) }
            return
        }
        remaining -= n
    }
}
filler.resume()

status = AudioDeviceStart(aggregateID, procID)
guard status == noErr else { fail("start", status, "启动采集失败") }
emit(["event": "started", "permission": Permission.preflight().0])

// MARK: - 退出处理

signal(SIGINT, SIG_IGN)
signal(SIGTERM, SIG_IGN)
let sigint = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
let sigterm = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
sigint.setEventHandler { shutdown(0) }
sigterm.setEventHandler { shutdown(0) }
sigint.resume()
sigterm.resume()

if args.contains("--watch-stdin") {
    Thread {
        var buf = [UInt8](repeating: 0, count: 256)
        while read(0, &buf, buf.count) > 0 {}
        DispatchQueue.main.async { shutdown(0) }
    }.start()
}

dispatchMain()
