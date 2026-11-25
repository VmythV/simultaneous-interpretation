// audio-tap — 听译的 macOS 音频辅助程序（需要 macOS 14.2+）
//
// 用法：
//   audio-tap [--app <bundleID>] [--watch-stdin]  采集电脑正在播放的声音（Core Audio Process Tap）；
//                                                  --app 只采集指定 App（含它的辅助进程）
//   audio-tap --mic [--aec] [--watch-stdin]       采集默认麦克风（AVAudioEngine）；--aec 开启系统回声消除
//   audio-tap --list-apps                         列出正在使用音频的 App（JSON）
//   audio-tap --meter ...                         调试：不输出 PCM，每 0.5 秒在 stderr 打印一次音量
//   audio-tap --check / --request                 检查 / 请求"系统录音"权限
//   audio-tap --mic-check / --mic-request         检查 / 请求麦克风权限
//   （权限都归属于启动本程序的 App）
//
// stdout：f32le 单声道 PCM，采样率见 format 事件
// stderr：每行一个 JSON 事件，例如
//   {"event":"format","sampleRate":48000,"channels":1}
//   {"event":"started","permission":"granted"}
//   {"event":"error","stage":"create-tap","code":-1,"message":"..."}
//
// 退出条件：SIGINT / SIGTERM、stdout 管道断开、--watch-stdin 时 stdin 被关闭（父进程退出）。

import AVFoundation
import AppKit
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

let argv = Array(CommandLine.arguments.dropFirst())
let args = Set(argv)
func argValue(_ name: String) -> String? {
    guard let i = argv.firstIndex(of: name), i + 1 < argv.count else { return nil }
    return argv[i + 1]
}
let meterMode = args.contains("--meter")
let debugMode = ProcessInfo.processInfo.environment["TINGYI_TAP_DEBUG"] != nil

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

func micState() -> String {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: return "granted"
    case .denied, .restricted: return "denied"
    case .notDetermined: return "undetermined"
    @unknown default: return "unknown"
    }
}

// MARK: - Core Audio 工具

func getProp<T>(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector, _ value: inout T) -> OSStatus {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var size = UInt32(MemoryLayout<T>.size)
    return withUnsafeMutablePointer(to: &value) { AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, $0) }
}

func defaultOutputDeviceUID() -> String {
    var device = AudioDeviceID(kAudioObjectUnknown)
    var st = getProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice, &device)
    guard st == noErr, device != kAudioObjectUnknown else { fail("default-output", st, "无法获取默认输出设备") }
    var uid: Unmanaged<CFString>?
    st = getProp(device, kAudioDevicePropertyDeviceUID, &uid)
    guard st == noErr, let value = uid?.takeRetainedValue() else { fail("output-uid", st, "无法获取输出设备 UID") }
    return value as String
}

/// 所有连接了 Core Audio 的进程对象
func processObjects() -> [AudioObjectID] {
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioHardwarePropertyProcessObjectList,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain)
    let system = AudioObjectID(kAudioObjectSystemObject)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(system, &addr, 0, nil, &size) == noErr, size > 0 else { return [] }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(system, &addr, 0, nil, &size, &ids) == noErr else { return [] }
    return ids
}

struct AudioProcess {
    let id: AudioObjectID
    let pid: pid_t
    let bundleID: String
    let isOutputting: Bool
}

func processInfo(_ id: AudioObjectID) -> AudioProcess? {
    var pid: pid_t = -1
    guard getProp(id, kAudioProcessPropertyPID, &pid) == noErr, pid > 0 else { return nil }
    var bid: Unmanaged<CFString>?
    var bundle = ""
    if getProp(id, kAudioProcessPropertyBundleID, &bid) == noErr, let b = bid?.takeRetainedValue() { bundle = b as String }
    var running: UInt32 = 0
    _ = getProp(id, kAudioProcessPropertyIsRunningOutput, &running)
    return AudioProcess(id: id, pid: pid, bundleID: bundle, isOutputting: running != 0)
}

// 辅助进程（Chrome Helper、Safari 的 WebKit 进程等）归到"负责"它的 App 名下
typealias ResponsibleFn = @convention(c) (pid_t) -> pid_t
let responsibleFn: ResponsibleFn? = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid")
    .map { unsafeBitCast($0, to: ResponsibleFn.self) }

/// 进程所属的 App（取不到 App 时返回 nil）
func owningApp(_ p: AudioProcess) -> NSRunningApplication? {
    if let f = responsibleFn {
        let r = f(p.pid)
        if r > 0, let app = NSRunningApplication(processIdentifier: r), app.bundleIdentifier != nil { return app }
    }
    if let app = NSRunningApplication(processIdentifier: p.pid), app.bundleIdentifier != nil { return app }
    return nil
}

/// 是否是听译自己（父进程）
func isSelf(_ app: NSRunningApplication) -> Bool {
    app.processIdentifier == getppid() || (app.bundleIdentifier ?? "").hasPrefix("app.tingyi")
}

/// 属于目标 App 的所有音频进程
func processes(of target: String) -> [AudioObjectID] {
    processObjects().compactMap { id in
        guard let p = processInfo(id) else { return nil }
        if let app = owningApp(p) {
            if isSelf(app) { return nil }
            return app.bundleIdentifier == target ? id : nil
        }
        return p.bundleID == target ? id : nil
    }
}

func iconBase64(_ app: NSRunningApplication) -> String {
    guard let icon = app.icon,
          let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 64, bitsPerSample: 8, samplesPerPixel: 4,
                                     hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
    else { return "" }
    rep.size = NSSize(width: 32, height: 32)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    icon.draw(in: NSRect(x: 0, y: 0, width: 32, height: 32))
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])?.base64EncodedString() ?? ""
}

func listApps() -> Never {
    var apps: [String: (name: String, playing: Bool, app: NSRunningApplication)] = [:]
    for id in processObjects() {
        guard let p = processInfo(id), let app = owningApp(p), !isSelf(app), let bid = app.bundleIdentifier else { continue }
        let playing = (apps[bid]?.playing ?? false) || p.isOutputting
        apps[bid] = (app.localizedName ?? bid, playing, app)
    }
    let list = apps
        .sorted { a, b in a.value.playing != b.value.playing ? a.value.playing : a.value.name.localizedStandardCompare(b.value.name) == .orderedAscending }
        .map { ["bundleId": $0.key, "name": $0.value.name, "playing": $0.value.playing, "icon": iconBase64($0.value.app)] as [String: Any] }
    emit(["event": "apps", "apps": list])
    exit(0)
}

// MARK: - 一次性命令

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
if args.contains("--list-apps") {
    listApps()
}

// MARK: - 输出 PCM（采集回调和静音补齐共用）

var stopping = false
var tapFrames = 0
var onShutdown: [() -> Void] = []
var meterSumSquares: Double = 0
var meterPeak: Float = 0
var meterCount = 0
var meterWindow = 24000

func shutdown(_ code: Int32) -> Never {
    stopping = true
    for f in onShutdown.reversed() { f() }
    exit(code)
}

/// 写出一段单声道采样；读取方关闭管道时退出
func output(_ samples: UnsafePointer<Float>, _ frames: Int) {
    if stopping || frames == 0 { return }
    if meterMode {
        for i in 0..<frames {
            let s = samples[i]
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
    if fwrite(samples, 4, frames, stdout) < frames {
        stopping = true
        DispatchQueue.main.async { shutdown(0) }
    }
}

// MARK: - 采集电脑声音（Process Tap）

func runSystemTap(target: String?) {
    var tapped: [AudioObjectID] = target.map { processes(of: $0) } ?? []
    let tapDescription = target == nil
        ? CATapDescription(monoGlobalTapButExcludeProcesses: [])
        : CATapDescription(monoMixdownOfProcesses: tapped)
    tapDescription.uuid = UUID()
    tapDescription.name = "TingYi System Audio"
    tapDescription.isPrivate = true
    tapDescription.muteBehavior = .unmuted

    var tapID = AudioObjectID(kAudioObjectUnknown)
    var status = AudioHardwareCreateProcessTap(tapDescription, &tapID)
    guard status == noErr else { fail("create-tap", status, "创建系统声音 Tap 失败") }
    onShutdown.append { AudioHardwareDestroyProcessTap(tapID) }

    var asbd = AudioStreamBasicDescription()
    status = getProp(tapID, kAudioTapPropertyFormat, &asbd)
    guard status == noErr else { fail("tap-format", status, "读取 Tap 音频格式失败") }
    guard asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0, asbd.mBitsPerChannel == 32 else {
        fail("tap-format", -1, "不支持的 Tap 格式：flags=\(asbd.mFormatFlags) bits=\(asbd.mBitsPerChannel)")
    }
    let sampleRate = asbd.mSampleRate
    meterWindow = Int(sampleRate / 2)

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
    guard status == noErr else { fail("create-aggregate", status, "创建聚合设备失败") }
    onShutdown.append { AudioHardwareDestroyAggregateDevice(aggregateID) }

    emit(["event": "format", "sampleRate": Int(sampleRate), "channels": 1])

    var mono = [Float](repeating: 0, count: 4096)
    // 没有任何声音在播放时系统不会回调；用补零保持输出是连续的时间轴，
    // 否则下游 VAD 等不到"静音"，最后一句永远无法断句。
    var lastWrite = DispatchTime.now()
    let zeros = [Float](repeating: 0, count: Int(sampleRate))
    let ioQueue = DispatchQueue(label: "app.tingyi.audio-tap", qos: .userInteractive)

    var procID: AudioDeviceIOProcID?
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
        lastWrite = DispatchTime.now()
        tapFrames += frames
        mono.withUnsafeBufferPointer { output($0.baseAddress!, frames) }
    }
    guard status == noErr, let procID else { fail("create-ioproc", status, "创建采集回调失败") }
    if debugMode {
        let stats = DispatchSource.makeTimerSource(queue: ioQueue)
        stats.schedule(deadline: .now() + 1, repeating: 1)
        stats.setEventHandler {
            var rate: Float64 = 0
            _ = getProp(aggregateID, kAudioDevicePropertyNominalSampleRate, &rate)
            var f = AudioStreamBasicDescription()
            _ = getProp(tapID, kAudioTapPropertyFormat, &f)
            diag("Tap 每秒收到 \(tapFrames) 帧，聚合设备 \(Int(rate))Hz，Tap 格式 \(Int(f.mSampleRate))Hz×\(f.mChannelsPerFrame)ch")
            tapFrames = 0
        }
        stats.resume()
        onShutdown.append { stats.cancel() }
    }
    onShutdown.append {
        AudioDeviceStop(aggregateID, procID)
        AudioDeviceDestroyIOProcID(aggregateID, procID)
    }

    // 静音补齐：与回调在同一个串行队列上，避免并发写 stdout
    let filler = DispatchSource.makeTimerSource(queue: ioQueue)
    filler.schedule(deadline: .now() + 0.05, repeating: 0.05)
    filler.setEventHandler {
        if stopping || meterMode { return }
        let now = DispatchTime.now()
        let gap = Double(now.uptimeNanoseconds - lastWrite.uptimeNanoseconds) / 1e9
        guard gap >= 0.15 else { return }
        var remaining = Int(gap * sampleRate)
        lastWrite = now
        while remaining > 0 && !stopping {
            let n = min(remaining, zeros.count)
            zeros.withUnsafeBufferPointer { output($0.baseAddress!, n) }
            remaining -= n
        }
    }
    filler.resume()

    status = AudioDeviceStart(aggregateID, procID)
    guard status == noErr else { fail("start", status, "启动采集失败") }
    var started: [String: Any] = ["event": "started", "permission": Permission.preflight().0]
    if let target { started["app"] = target; started["processes"] = tapped.count }
    emit(started)

    // 只听指定 App：它的音频进程可能晚一点才出现（或新开辅助进程）。
    // 监听系统的进程列表变化，新进程一建立音频连接就加入 Tap（避免丢掉开头）；另每 2 秒兜底检查一次
    if let target {
        let t0 = DispatchTime.now()
        let elapsed = { (Double(DispatchTime.now().uptimeNanoseconds - t0.uptimeNanoseconds) / 1e6).rounded() / 1000 }
        let refresh = { (why: String) in
            let now = processes(of: target)
            if debugMode {
                let pids = now.compactMap { processInfo($0).map { "\($0.id):\($0.pid)\($0.isOutputting ? "*" : "")" } }
                emit(["event": "debug", "message": "t=\(elapsed())s \(why) 匹配 \(pids)，当前 Tap \(tapped)"])
            }
            guard Set(now) != Set(tapped) else { return }
            tapDescription.processes = now
            var addr = AudioObjectPropertyAddress(
                mSelector: kAudioTapPropertyDescription,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain)
            // 属性值是对象指针（CATapDescription *）
            var desc = tapDescription
            let st = withUnsafePointer(to: &desc) {
                AudioObjectSetPropertyData(tapID, &addr, 0, nil, UInt32(MemoryLayout<CATapDescription>.size), $0)
            }
            if st == noErr {
                tapped = now
                emit(["event": "processes", "app": target, "processes": now.count])
            } else {
                emit(["event": "warning", "stage": "update-tap", "code": Int(st), "message": "更新要采集的进程失败"])
            }
        }
        var listAddr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyProcessObjectList,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        let listener: AudioObjectPropertyListenerBlock = { _, _ in refresh("进程列表变化") }
        AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &listAddr, .main, listener)
        let refresher = DispatchSource.makeTimerSource(queue: .main)
        refresher.schedule(deadline: .now() + 2, repeating: 2)
        refresher.setEventHandler { refresh("定时检查") }
        refresher.resume()
        onShutdown.append {
            refresher.cancel()
            AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &listAddr, .main, listener)
        }
    }
    onShutdown.append { filler.cancel() }
}

// MARK: - 采集麦克风（AVAudioEngine，可开启系统回声消除）

var micEngine: AVAudioEngine?
var micReady = false

func diag(_ message: String) {
    if debugMode { emit(["event": "debug", "message": message]) }
}

func describe(_ f: AVAudioFormat) -> String {
    "\(Int(f.sampleRate))Hz×\(f.channelCount)ch\(f.isInterleaved ? "(交错)" : "")"
}

/// 普通麦克风采集（不开回声消除；桌面应用的普通模式用 cpal，这里主要用于测试）
func startMicEngine() throws -> AVAudioEngine {
    let engine = AVAudioEngine()
    let input = engine.inputNode
    let format = input.outputFormat(forBus: 0)
    diag("麦克风 \(describe(format))")
    guard format.sampleRate > 0, format.channelCount > 0 else {
        throw NSError(domain: "tingyi", code: -1, userInfo: [NSLocalizedDescriptionKey: "没有可用的麦克风"])
    }
    var mono = [Float](repeating: 0, count: 4096)
    input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
        guard micReady, let data = buffer.floatChannelData else { return }
        let frames = Int(buffer.frameLength)
        let nch = Int(buffer.format.channelCount)
        if nch == 1 {
            output(data[0], frames)
            return
        }
        if mono.count < frames { mono = [Float](repeating: 0, count: frames) }
        for i in 0..<frames {
            var sum: Float = 0
            for c in 0..<nch { sum += data[c][i] }
            mono[i] = sum / Float(nch)
        }
        mono.withUnsafeBufferPointer { output($0.baseAddress!, frames) }
    }
    engine.prepare()
    try engine.start()
    return engine
}

/// 直接使用 Voice Processing 音频单元（AVAudioEngine 开启回声消除后输出 9 声道，在部分机器上初始化失败 -10875）。
/// 我们只取处理后的单声道人声；输出端送静音，回声消除以扬声器实际播放的声音作参考。
final class VoiceIO {
    var unit: AudioUnit?
    var buffer = [Float](repeating: 0, count: 8192)
    let sampleRate: Double

    init(sampleRate: Double) {
        self.sampleRate = sampleRate
    }

    private func check(_ status: OSStatus, _ step: String) throws {
        if status != noErr {
            throw NSError(domain: "VoiceIO", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "\(step) 失败（\(status)）"])
        }
    }

    func start() throws {
        var desc = AudioComponentDescription(
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_VoiceProcessingIO,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0, componentFlagsMask: 0)
        guard let comp = AudioComponentFindNext(nil, &desc) else { try check(-1, "找不到回声消除组件") ; return }
        var u: AudioUnit?
        try check(AudioComponentInstanceNew(comp, &u), "创建回声消除单元")
        guard let unit = u else { return }
        self.unit = unit

        var one: UInt32 = 1
        let u32 = UInt32(MemoryLayout<UInt32>.size)
        try check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, 1, &one, u32), "开启麦克风输入")
        try check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, 0, &one, u32), "开启输出")

        var fmt = AudioStreamBasicDescription(
            mSampleRate: sampleRate, mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4, mFramesPerPacket: 1, mBytesPerFrame: 4, mChannelsPerFrame: 1, mBitsPerChannel: 32, mReserved: 0)
        let fmtSize = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        try check(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 1, &fmt, fmtSize), "设置麦克风格式")
        try check(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &fmt, fmtSize), "设置输出格式")

        let cbSize = UInt32(MemoryLayout<AURenderCallbackStruct>.size)
        var inputCb = AURenderCallbackStruct(inputProc: voiceInputCallback, inputProcRefCon: Unmanaged.passUnretained(self).toOpaque())
        try check(AudioUnitSetProperty(unit, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global, 1, &inputCb, cbSize), "设置采集回调")
        var renderCb = AURenderCallbackStruct(inputProc: silenceCallback, inputProcRefCon: nil)
        try check(AudioUnitSetProperty(unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &renderCb, cbSize), "设置输出回调")

        // 回声消除会压低其他 App 的音量（ducking），调到最小，避免影响"对方"的录音和你听到的音量
        var duck = AUVoiceIOOtherAudioDuckingConfiguration(mEnableAdvancedDucking: false, mDuckingLevel: .min)
        let st = AudioUnitSetProperty(unit, kAUVoiceIOProperty_OtherAudioDuckingConfiguration, kAudioUnitScope_Global, 0,
                                      &duck, UInt32(MemoryLayout<AUVoiceIOOtherAudioDuckingConfiguration>.size))
        if st != noErr { diag("设置 ducking 失败：\(st)") }

        try check(AudioUnitInitialize(unit), "初始化回声消除")
        try check(AudioOutputUnitStart(unit), "启动回声消除")
    }

    func render(_ flags: UnsafeMutablePointer<AudioUnitRenderActionFlags>, _ ts: UnsafePointer<AudioTimeStamp>, _ frames: UInt32) -> OSStatus {
        guard let unit else { return noErr }
        let n = Int(frames)
        if buffer.count < n { return kAudio_ParamError }
        return buffer.withUnsafeMutableBufferPointer { ptr -> OSStatus in
            var abl = AudioBufferList(
                mNumberBuffers: 1,
                mBuffers: AudioBuffer(mNumberChannels: 1, mDataByteSize: UInt32(n * 4), mData: UnsafeMutableRawPointer(ptr.baseAddress)))
            let st = AudioUnitRender(unit, flags, ts, 1, frames, &abl)
            if st == noErr, micReady { output(ptr.baseAddress!, n) }
            return st
        }
    }

    func stop() {
        guard let unit else { return }
        AudioOutputUnitStop(unit)
        AudioUnitUninitialize(unit)
        AudioComponentInstanceDispose(unit)
        self.unit = nil
    }
}

let voiceInputCallback: AURenderCallback = { refCon, flags, ts, _, frames, _ in
    Unmanaged<VoiceIO>.fromOpaque(refCon).takeUnretainedValue().render(flags, ts, frames)
}

let silenceCallback: AURenderCallback = { _, flags, _, _, _, ioData in
    if let abl = UnsafeMutableAudioBufferListPointer(ioData) {
        for b in abl where b.mData != nil { memset(b.mData, 0, Int(b.mDataByteSize)) }
    }
    flags.pointee.insert(.unitRenderAction_OutputIsSilence)
    return noErr
}

var voiceIO: VoiceIO?

func runVoiceIO() -> Bool {
    let io = VoiceIO(sampleRate: 48000)
    do {
        try io.start()
    } catch {
        emit(["event": "debug", "message": "回声消除单元：\((error as NSError).localizedDescription)"])
        io.stop()
        return false
    }
    voiceIO = io
    meterWindow = 24000
    emit(["event": "format", "sampleRate": 48000, "channels": 1, "aec": true])
    micReady = true
    onShutdown.append { io.stop() }
    emit(["event": "started", "permission": micState(), "aec": true, "strategy": "voice-io"])
    return true
}

func runMic(aec: Bool) {
    if aec {
        // AVAudioEngine 开启回声消除后输出 9 声道，在部分机器上初始化失败（-10875），所以直接用音频单元
        if !runVoiceIO() { fail("mic-start", -1, "启动系统回声消除失败") }
        return
    }
    do {
        let engine = try startMicEngine()
        micEngine = engine
        let format = engine.inputNode.outputFormat(forBus: 0)
        meterWindow = Int(format.sampleRate / 2)
        emit(["event": "format", "sampleRate": Int(format.sampleRate), "channels": 1, "aec": false])
        micReady = true
        onShutdown.append {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
        }
        emit(["event": "started", "permission": micState(), "aec": false])
    } catch {
        fail("mic-start", Int32((error as NSError).code), "启动麦克风失败：\(error.localizedDescription)")
    }
}

// MARK: - 启动

if args.contains("--mic") {
    runMic(aec: args.contains("--aec"))
} else {
    runSystemTap(target: argValue("--app"))
}

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
