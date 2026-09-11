import MetalKit
import FieldCore

/// Independent lighting cache. All geometry is rasterized; these small compute
/// passes integrate atmospheric scattering only when the environment changes.
final class Atmosphere {
    let device:MTLDevice
    var weather=MoistWeather(seed:4)
    var weatherTexture:MTLTexture
    var weatherTime:Float=0
    var weatherBounds:SIMD4<Float> = .zero
    let densityMaxLevels:[MTLTexture]
    let emptyCells:MTLTexture
    var lastUniforms:Uniforms?
    let verifyPipeline:MTLComputePipelineState
    let maxPipeline:MTLComputePipelineState
    let emptyPipeline:MTLComputePipelineState
    var previousPublishedTime:Float=0
    var trans:MTLTexture
    var multiple:MTLTexture
    var sky:MTLTexture
    var irradiance:MTLTexture
    var previousSky:MTLTexture
    var pendingSky:MTLTexture
    var previousIrradiance:MTLTexture
    var pendingIrradiance:MTLTexture
    let clearSky:MTLTexture
    let cloudAmbient:MTLTexture
    let airRadiance:MTLTexture
    let airForeground:MTLTexture
    let airTransmission:MTLTexture
    let airPipeline:MTLComputePipelineState
    let cloudDensity:MTLTexture
    let cloudLighting:MTLTexture
    let cloudDensityPipeline:MTLComputePipelineState
    let cloudLightingPipeline:MTLComputePipelineState
    var updatePhase="idle"
    var cloudPhase=0
    var row=0
    var sampleTime:Float=0
    var publishedTime:Float=0
    var transitionTime:Float=0
    var transitionDuration:Float=4.27
    var wasPaused=true
    let transPipeline:MTLComputePipelineState
    let multiplePipeline:MTLComputePipelineState
    let skyPipeline:MTLComputePipelineState
    let irradiancePipeline:MTLComputePipelineState
    var noise:MTLTexture
    let noisePipeline:MTLComputePipelineState
    var key=""
    var densityKey=""
    var haze:Float = -1
    init(device:MTLDevice,library:MTLLibrary) throws {
        self.device=device
        let wd=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba32Float,width:64,height:64,mipmapped:false)
        wd.storageMode = .shared;wd.usage = .shaderRead
        weatherTexture=device.makeTexture(descriptor:wd)!
        func texture(_ w:Int,_ h:Int)->MTLTexture {
            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba16Float,width:w,height:h,mipmapped:false)
            d.usage=[.shaderRead,.shaderWrite];d.storageMode = .private;return device.makeTexture(descriptor:d)!
        }
        trans=texture(256,64);multiple=texture(32,32);sky=texture(8192,1024);irradiance=texture(64,32)
        previousSky=texture(8192,1024);pendingSky=texture(8192,1024)
        previousIrradiance=texture(64,32);pendingIrradiance=texture(64,32)
        airRadiance=texture(512,256);airForeground=texture(512,256);airTransmission=texture(512,256)
        clearSky=texture(128,64);cloudAmbient=texture(64,32)
        let nd=MTLTextureDescriptor();nd.textureType = .type3D;nd.width=128;nd.height=128;nd.depth=128;nd.pixelFormat = .rg16Float;nd.usage=[.shaderRead,.shaderWrite];nd.storageMode = .private
        noise=device.makeTexture(descriptor:nd)!
        nd.width=1024;nd.height=96;nd.depth=1024;nd.pixelFormat = .r16Float
        cloudDensity=device.makeTexture(descriptor:nd)!
        var levels:[MTLTexture]=[]
        for (w,h) in [(512,48),(256,24),(128,12)] {nd.width=w;nd.height=h;nd.depth=w;levels.append(device.makeTexture(descriptor:nd)!)}
        densityMaxLevels=levels
        emptyCells=device.makeTexture(descriptor:nd)!
        verifyPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereVerifyEmptySpace")!)
        maxPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereMaxDensity")!)
        emptyPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereEmptyCells")!)
        nd.width=256;nd.height=64;nd.depth=256;nd.pixelFormat = .r16Float
        cloudLighting=device.makeTexture(descriptor:nd)!
        cloudDensityPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereCloudDensity")!)
        cloudLightingPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereCloudLighting")!)
        airPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereAir")!)
        noisePipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereNoise")!)
        transPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereTrans")!)
        multiplePipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereMultiple")!)
        skyPipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereSky")!)
        irradiancePipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"atmosphereIrradiance")!)
    }
    func verifyEmptySpace(_ queue:MTLCommandQueue) throws ->[String:Any] {
        guard var u=lastUniforms else {throw RuntimeError.message("Render the sky before verifying it")}
        let output=device.makeBuffer(length:16,options:.storageModeShared)!
        memset(output.contents(),0,16)
        let cb=queue.makeCommandBuffer()!,e=cb.makeComputeCommandEncoder()!
        e.setComputePipelineState(verifyPipeline)
        for (i,t) in [cloudDensity,noise,weatherTexture,emptyCells].enumerated(){e.setTexture(t,index:i)}
        e.setBytes(&u,length:MemoryLayout<Uniforms>.stride,index:0);e.setBuffer(output,offset:0,index:1)
        e.dispatchThreads(MTLSize(width:131072,height:1,depth:1),threadsPerThreadgroup:MTLSize(width:64,height:1,depth:1));e.endEncoding()
        cb.commit();cb.waitUntilCompleted();if let error=cb.error {throw error}
        let values=output.contents().bindMemory(to:UInt32.self,capacity:4)
        return ["passed":values[1]==0,"candidateRays":131072,"certifiedSegments":values[0],"densitySamples":values[0]*9,"violations":values[1],"maximumSkippedDensity":Float(bitPattern:values[2]),"time":u.cameraTime.w]
    }
    func updateWeather(_ time:Float,_ sun:Float,reset:Float?=nil) {
        if let seed=reset {weather=MoistWeather(seed:UInt64(seed));weatherTime=0}
        weather.advance(to:time,sun:sun);weatherTime=Float(weather.ticks)*2
        // A fresh immutable snapshot avoids CPU writes racing in-flight GPU work.
        let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba32Float,width:64,height:64,mipmapped:false)
        d.storageMode = .shared;d.usage = .shaderRead
        let texture=device.makeTexture(descriptor:d)!,cells=weather.gpuCells
        cells.withUnsafeBytes {texture.replace(region:MTLRegionMake2D(0,0,64,64),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:64*16)}
        weatherTexture=texture
        var gx:Float=0,gz:Float=0
        for z in 0..<64 {for x in 0..<64 {
            let w=cells[z*64+x].z
            gx=max(gx,abs(w-cells[z*64+(x+1)%64].z));gz=max(gz,abs(w-cells[((z+1)%64)*64+x].z))
        }}
        weatherBounds=SIMD4(cells.map(\.x).max() ?? 0,gx*0.035,gz*0.035,0)
    }
    func encode(_ cb:MTLCommandBuffer,_ u:inout Uniforms,paused:Bool,profile:GPUProfile?=nil) {
        let next="\(u.options.y)-\(u.environment.x)-\(u.sunExposure.x)-\(u.sunExposure.y)-\(u.sunExposure.z)-\(u.sky)"
        let nextDensity="\(u.sky.x)-\(u.sky.y)-\(u.sky.w)"
        let densityChanged=densityKey != nextDensity;densityKey=nextDensity
        let changed=key != next,now=u.cameraTime.w
        u.weatherBounds=weatherBounds
        let full=changed || (paused && (!wasPaused || publishedTime != now))
        updatePhase="idle"
        wasPaused=paused
        let first=key.isEmpty,mediumChanged=haze != u.sky.z;key=next;haze=u.sky.z
        if first {
            let e=profile?.compute(cb,"sky.noise") ?? cb.makeComputeCommandEncoder()!;e.setComputePipelineState(noisePipeline);e.setTexture(noise,index:0)
            e.dispatchThreads(MTLSize(width:128,height:128,depth:128),threadsPerThreadgroup:MTLSize(width:4,height:4,depth:4));e.endEncoding()
        }
        func dispatch(_ pipeline:MTLComputePipelineState,_ textures:[MTLTexture],_ output:MTLTexture,rows:Int?=nil) {
            let e=profile?.compute(cb,pipeline === airPipeline ? "sky.air" : (pipeline === skyPipeline ? "sky.rays" : (pipeline === irradiancePipeline ? "sky.irradiance" : (pipeline === transPipeline ? "sky.transmittance" : "sky.multiple")))) ?? cb.makeComputeCommandEncoder()!;e.setComputePipelineState(pipeline)
            for(i,t) in textures.enumerated(){e.setTexture(t,index:i)}
            e.setBytes(&u,length:MemoryLayout<Uniforms>.stride,index:0)
            e.dispatchThreads(MTLSize(width:output.width,height:rows ?? output.height,depth:1),threadsPerThreadgroup:pipeline === skyPipeline ? MTLSize(width:64,height:1,depth:1):MTLSize(width:8,height:8,depth:1));e.endEncoding()
        }
        func volume(_ pipeline:MTLComputePipelineState,_ input:MTLTexture,_ output:MTLTexture,offset:Int=0,slices:Int?=nil) {
            let count=slices ?? output.height,isLighting=pipeline === cloudLightingPipeline
            let dispatchCount=isLighting ? count:1
            for pass in 0..<dispatchCount {
                u.environment.w=Float(offset+(isLighting ? pass:0))
                let e=profile?.compute(cb,isLighting ? "sky.cloudLighting" : "sky.density") ?? cb.makeComputeCommandEncoder()!;e.setComputePipelineState(pipeline)
                e.setTexture(input,index:0);e.setTexture(output,index:1);e.setTexture(noise,index:2);e.setTexture(weatherTexture,index:3)
                e.setBytes(&u,length:MemoryLayout<Uniforms>.stride,index:0)
                e.dispatchThreads(MTLSize(width:output.width,height:isLighting ? 1:count,depth:output.depth),threadsPerThreadgroup:MTLSize(width:8,height:1,depth:8));e.endEncoding()
            }
            u.environment.w=0
        }
        if mediumChanged {dispatch(transPipeline,[trans],trans);dispatch(multiplePipeline,[trans,multiple],multiple)}
        if changed {
            let coverage=u.sky.x;u.sky.x=0;u.environment.w=0
            dispatch(skyPipeline,[trans,multiple,clearSky,noise,cloudAmbient,cloudDensity,cloudLighting,airRadiance,airForeground,airTransmission,weatherTexture,emptyCells],clearSky)
            dispatch(irradiancePipeline,[clearSky,cloudAmbient],cloudAmbient)
            u.sky.x=coverage
        }
        if full {
            updateWeather(now*u.options.y,u.sunExposure.y,reset:changed ? u.sky.w:nil);u.weatherBounds=weatherBounds
            updatePhase="rebuild"
            if densityChanged {
                u.cameraTime.w=0;volume(cloudDensityPipeline,noise,cloudDensity);u.cameraTime.w=now
                var source=cloudDensity
                for target in densityMaxLevels {volume(maxPipeline,source,target);source=target}
                volume(emptyPipeline,source,emptyCells)
            }
            volume(cloudLightingPipeline,cloudDensity,cloudLighting)
            u.environment.w=0
            dispatch(airPipeline,[trans,multiple,airRadiance,airForeground,airTransmission,cloudLighting],airRadiance)
            dispatch(skyPipeline,[trans,multiple,sky,noise,cloudAmbient,cloudDensity,cloudLighting,airRadiance,airForeground,airTransmission,weatherTexture,emptyCells],sky)
            dispatch(irradiancePipeline,[sky,irradiance],irradiance)
            let b=cb.makeBlitCommandEncoder()!
            b.copy(from:sky,to:previousSky);b.copy(from:sky,to:pendingSky);b.copy(from:irradiance,to:previousIrradiance);b.endEncoding()
            row=0;cloudPhase=0;previousPublishedTime=now;publishedTime=now;transitionTime=now-1
        } else if !paused && u.environment.x != 4 {
            // Build density, optical depth and the angular snapshot in bounded
            // slices. Only completed sky/irradiance pairs become visible.
            if row==0 && cloudPhase==0 {sampleTime=now;updateWeather(now*u.options.y,u.sunExposure.y);u.weatherBounds=weatherBounds}
            u.cameraTime.w=sampleTime
            // Low sun has longer primary and secondary paths. Reprojection
            // carries cloud motion between less frequent complete snapshots.
            let lightSlices=u.sunExposure.y<0.26 ? 1:2
            let skyRows=u.sunExposure.y<0.26 ? 2:4
            if cloudPhase<64 {
                updatePhase="lighting"
                volume(cloudLightingPipeline,cloudDensity,cloudLighting,offset:cloudPhase,slices:lightSlices)
                cloudPhase+=lightSlices
            } else if cloudPhase<72 {
                updatePhase="air";u.environment.w=Float(128+(cloudPhase-64)*16)
                dispatch(airPipeline,[trans,multiple,airRadiance,airForeground,airTransmission,cloudLighting],airRadiance,rows:16)
                cloudPhase+=1
            } else {
                updatePhase="sky"
                u.environment.w=Float(row)
                dispatch(skyPipeline,[trans,multiple,pendingSky,noise,cloudAmbient,cloudDensity,cloudLighting,airRadiance,airForeground,airTransmission,weatherTexture,emptyCells],pendingSky,rows:skyRows)
                row+=skyRows
                if row==1024 {
                    dispatch(irradiancePipeline,[pendingSky,pendingIrradiance],pendingIrradiance)
                    swap(&previousSky,&sky);swap(&sky,&pendingSky)
                    swap(&previousIrradiance,&irradiance);swap(&irradiance,&pendingIrradiance)
                    row=0;cloudPhase=0;previousPublishedTime=publishedTime;publishedTime=sampleTime;transitionDuration=min(1.5,max(0.1,now-transitionTime));transitionTime=now
                }
            }
        }
        lastUniforms=u
        u.cameraTime.w=now;u.environment.w=0
        u.skyTimes=SIMD4(previousPublishedTime,publishedTime,0,0)
        u.environment.z=paused ? 1:min(1,max(0,(now-transitionTime)/transitionDuration))
    }
}
