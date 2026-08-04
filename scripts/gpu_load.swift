import Foundation
import Metal

struct Options {
    var durationSeconds = 10.0
    var dutyCycle = 1.0
    var cycleMilliseconds = 100.0
}

func parseOptions() throws -> Options {
    var options = Options()
    var arguments = CommandLine.arguments.dropFirst().makeIterator()
    while let argument = arguments.next() {
        guard let value = arguments.next(), let number = Double(value) else {
            throw NSError(
                domain: "rasitop-gpu-load",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "(argument) requires a numeric value"]
            )
        }
        switch argument {
        case "--duration-seconds": options.durationSeconds = number
        case "--duty-cycle": options.dutyCycle = number
        case "--cycle-milliseconds": options.cycleMilliseconds = number
        default:
            throw NSError(
                domain: "rasitop-gpu-load",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "unknown argument (argument)"]
            )
        }
    }
    guard options.durationSeconds > 0,
          (0 ... 1).contains(options.dutyCycle),
          options.cycleMilliseconds > 0
    else {
        throw NSError(
            domain: "rasitop-gpu-load",
            code: 2,
            userInfo: [NSLocalizedDescriptionKey: "duration and cycle must be positive; duty cycle must be in 0...1"]
        )
    }
    return options
}

let shader = """
#include <metal_stdlib>
using namespace metal;

kernel void rasitop_gpu_load(
    device float *values [[buffer(0)]],
    uint index [[thread_position_in_grid]])
{
    float value = values[index];
    for (uint iteration = 0; iteration < 1024; iteration++) {
        value = fma(value, 1.000000119f, 0.000000119f);
        value = sqrt(value * value + 1.0f);
    }
    values[index] = value;
}
"""

func run() throws {
    let options = try parseOptions()
    guard let device = MTLCreateSystemDefaultDevice(),
          let queue = device.makeCommandQueue()
    else {
        throw NSError(
            domain: "rasitop-gpu-load",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "Metal device or command queue unavailable"]
        )
    }
    let library = try device.makeLibrary(source: shader, options: nil)
    guard let function = library.makeFunction(name: "rasitop_gpu_load") else {
        throw NSError(
            domain: "rasitop-gpu-load",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "load kernel unavailable"]
        )
    }
    let pipeline = try device.makeComputePipelineState(function: function)
    let elementCount = 1 << 16
    guard let buffer = device.makeBuffer(
        length: elementCount * MemoryLayout<Float>.stride,
        options: .storageModeShared
    ) else {
        throw NSError(
            domain: "rasitop-gpu-load",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "Metal buffer allocation failed"]
        )
    }
    buffer.contents().bindMemory(to: Float.self, capacity: elementCount)
        .initialize(repeating: 1.0, count: elementCount)

    let clock = ContinuousClock()
    let end = clock.now.advanced(by: .milliseconds(Int64(options.durationSeconds * 1_000)))
    let cycle = Duration.milliseconds(Int64(options.cycleMilliseconds))
    let active = Duration.milliseconds(Int64(options.cycleMilliseconds * options.dutyCycle))
    while clock.now < end {
        let cycleStart = clock.now
        let activeEnd = cycleStart.advanced(by: active)
        while clock.now < activeEnd, clock.now < end {
            try autoreleasepool {
                guard let commandBuffer = queue.makeCommandBuffer(),
                      let encoder = commandBuffer.makeComputeCommandEncoder()
                else {
                    throw NSError(
                        domain: "rasitop-gpu-load",
                        code: 1,
                        userInfo: [NSLocalizedDescriptionKey: "Metal command creation failed"]
                    )
                }
                encoder.setComputePipelineState(pipeline)
                encoder.setBuffer(buffer, offset: 0, index: 0)
                let width = pipeline.threadExecutionWidth
                encoder.dispatchThreads(
                    MTLSize(width: elementCount, height: 1, depth: 1),
                    threadsPerThreadgroup: MTLSize(width: width, height: 1, depth: 1)
                )
                encoder.endEncoding()
                commandBuffer.commit()
                commandBuffer.waitUntilCompleted()
                if let error = commandBuffer.error {
                    throw error
                }
            }
        }
        let cycleEnd = cycleStart.advanced(by: cycle)
        if clock.now < cycleEnd, clock.now < end {
            Thread.sleep(forTimeInterval: min(
                Double((cycleEnd - clock.now).components.attoseconds) / 1e18,
                Double((end - clock.now).components.attoseconds) / 1e18
            ))
        }
    }
}

do {
    try run()
} catch {
    FileHandle.standardError.write(Data("rasitop-gpu-load: \(error.localizedDescription)\n".utf8))
    exit(1)
}
