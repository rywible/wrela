import Foundation

extension MetalRenderer {
  package func diagnostics() -> [String: Any] {
    [
      "sourceRevision": Bundle.main.object(forInfoDictionaryKey: "SanctuaryRevision") as? String
        ?? "unbundled",
      "sourceDigest": Bundle.main.object(forInfoDictionaryKey: "SanctuarySourceDigest") as? String
        ?? "unbundled",
      "frame": frame, "antialiasing": "4x MSAA", "gpuErrors": gpuErrors,
      "shaderDigest": shaderDigest,
      "renderer": useMeshlets
        ? "Field-derived meshes / Metal mesh shaders" : "Field-derived meshes / indexed raster",
      "renderSize": [1920, 1080], "device": device.name,
      "thermalState": ProcessInfo.processInfo.thermalState.rawValue,
      "lowPowerMode": ProcessInfo.processInfo.isLowPowerModeEnabled,
      "skySnapshotTimes": [atmosphere.previousPublishedTime, atmosphere.publishedTime],
      "gpuPassSampleCounts": gpuPassTimes.mapValues(\.count), "profiling": profiling,
      "gpuCountersSupported": GPUProfile.supported(device),
      "gpuPassMilliseconds": gpuPassTimes.mapValues { stats($0) },
      "gpuMilliseconds": stats(gpuTimes),
      "frameGPUBySkyPhase": gpuPhaseTimes.mapValues { stats($0) },
      "skyUpdatePhase": atmosphere.updatePhase,
      "weather": [
        "model": "moist columns", "seconds": atmosphere.weatherTime,
        "totalWater": atmosphere.weather.totalWater, "evaporated": atmosphere.weather.evaporated,
        "precipitated": atmosphere.weather.precipitated,
      ], "cpuEncodeMilliseconds": stats(cpuTimes), "frameIntervalMilliseconds": stats(frameTimes),
      "cpuSimulationMilliseconds": stats(simulationTimes),
      "cpuAtmosphereMilliseconds": stats(atmosphereCPUTimes),
      "submissionToCompletionMilliseconds": stats(completionTimes),
      "presentedIntervalMilliseconds": stats(presentedIntervals),
      "presentedFrames": presentedFrames,
      "drawableWaitMilliseconds": stats(drawableWaitTimes), "visibleTriangles": visibleTriangles,
      "shadowTriangles": shadowTriangles,
      "timingSampleCount": gpuTimes.count, "allocatedMetalBytes": device.currentAllocatedSize,
      "skyCacheSize": [atmosphere.sky.width, atmosphere.sky.height],
    ]
  }
}
