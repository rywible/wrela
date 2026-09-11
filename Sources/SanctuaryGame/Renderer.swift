import AppKit
import MetalKit
import simd
import CryptoKit
import FieldCore
import FieldCompiler

struct Uniforms {
    var viewProjection: simd_float4x4
    var lightVP: simd_float4x4
    var inverseVP: simd_float4x4
    var cameraTime: SIMD4<Float>
    var sunExposure: SIMD4<Float>
    var options: SIMD4<Float>
    var sky: SIMD4<Float>
    var environment: SIMD4<Float>
    var weatherBounds:SIMD4<Float> = .zero
    var skyTimes:SIMD4<Float> = .zero
    var rigPosition:SIMD4<Float> = .zero
    var rigColor:SIMD4<Float> = .zero
    var rigParams:SIMD4<Float> = .zero
    var lookSurface:SIMD4<Float> = SceneLook().surface
    var lookLight:SIMD4<Float> = SceneLook().light
    var lookGrade:SIMD4<Float> = SceneLook().grade
}

struct GPUBatch {
    var name: String
    var vertices: MTLBuffer
    var indices: MTLBuffer
    var instances: MTLBuffer
    var indexCount: Int
    var instanceCount: Int
    var sourceInstances: [Instance]
    var visibleBuffers: [MTLBuffer]
    var center: V3
    var radius: Float
    var lods:[GPUBatch]
    var error:Float
    var meshlets:MTLBuffer
    var meshletVertices:MTLBuffer
    var meshletTriangles:MTLBuffer
    var material:SIMD4<Float> = SIMD4(-1,0,0,0)
    var meshletCount:Int
}

final class GardenRenderer: NSObject, MTKViewDelegate {
    let isSoundstage:Bool
    let device: MTLDevice
    let queue: MTLCommandQueue
    var world: GardenWorld
    var atmosphere: Atmosphere!
    var post:PostProcess!
    var meshPipeline: MTLRenderPipelineState!
    var materialPipelines:[Int:MTLRenderPipelineState]=[:]
    var materialMeshPipelines:[Int:MTLRenderPipelineState]=[:]
    var useMeshlets=true
    var windBuffers:[MTLBuffer]=[]
    var lodSelection:[String:[Int:Int]]=[:]
    var wetness:Float=0
    var atmosphereWind=WindSimulation()
    let view: MTKView
    var batches:[GPUBatch]=[]
    var pod: GPUBatch!
    var floor: GPUBatch!
    var pipeline: MTLRenderPipelineState!
    var skyPipeline: MTLRenderPipelineState!
    var groundPipeline: MTLRenderPipelineState!
    var shadowPipeline: MTLRenderPipelineState!
    var depthState: MTLDepthStencilState!
    var skyDepthState: MTLDepthStencilState!
    var shadowTexture: MTLTexture!
    var shadowTexelsPerMetre:Float=1
    var camera=V3(0,0,24)
    var yaw:Float = 0
    var pitch:Float = -0.04
    var studioObject="seed"
    var studioViewName="Custom"
    var studioSource=AssetSource.defaults("seed")
    var importedSourceURL:URL?
    var sceneLook=(try? SceneLook.load()) ?? SceneLook()
    lazy var workshopSession=WorkshopSession(self)
    var studioBatches:[GPUBatch]=[]
    var studioCache:[String:[GPUBatch]]=[:]
    var studioBounds=Bounds(V3(-0.01,0,-0.01),V3(0.01,0.035,0.01))
    var studioRig=StudioRig()
    var studioLayout=StudioLayout()
    var studioBuildMilliseconds:Double=0
    var studioUnit:Float {studioObject=="sky" ? 1:max(0.001,studioExtent/4)}
    var scene="garden"
    func setStudyView(_ name:String) {
        studioViewName=name;scene="studio";orbitDistance=6.8;studioLayout.focus=0.5
        if !["sunward","away","zenith"].contains(name) {studioObject=studioSource.id}
        switch name {
        case "sunward","away","zenith":
            studioObject="sky";orbit=skySettings.azimuth * .pi/180+(name=="away" ? .pi:0)
            orbitElevation=name=="zenith" ? 1.5:0.48
        case "back":orbit = .pi;orbitElevation=0.2
        case "left":orbit = -.pi/2;orbitElevation=0.2
        case "right":orbit = .pi/2;orbitElevation=0.2
        case "quarter":orbit=0.8;orbitElevation=0.2
        case "above":orbit=0.5;orbitElevation=1.0
        case "detail","base","crown":orbit=0.6;orbitElevation=0.2;orbitDistance=2;studioLayout.focus=name=="base" ? 0.12:(name=="crown" ? 0.8:0.5)
        default:orbit=0;orbitElevation=0.2
        }
        if !["detail","base","crown","sunward","away","zenith"].contains(name) {fitStudioCamera()}
    }
    var skySettings=SkySettings.preset("morning")
    var lighting="morning" {didSet {skySettings=SkySettings.preset(lighting);studioRig=StudioRig.preset(lighting=="indoor" ? "room":"outdoor")}}
    var exposure:Float=1
    var wind:Float=1
    var podScale:Float=1
    var orbit:Float=0.48
    var orbitElevation:Float=0.18
    var orbitDistance:Float=6.8
    var paused=false
    var simulationTime:Float=0
    var keys=Set<UInt16>()
    var frame=0
    var gpuTimes:[Double]=[]
    var gpuPhaseTimes:[String:[Double]]=[:]
    var gpuPassTimes:[String:[Double]]=[:]
    var profiling=false
    var metricsGeneration=0
    var cpuTimes:[Double]=[]
    var frameTimes:[Double]=[]
    var drawableWaitTimes:[Double]=[]
    var visibleTriangles=0
    var shadowTriangles=0
    private var cullMatrix=matrix_identity_float4x4
    private var measuredScene="garden"
    private var accumulator:Double=0
    var shaderDigest=""
    var gpuErrors:[String]=[]
    var lastTime=CACurrentMediaTime()
    var onUpdate:(()->Void)?
    var captureRequest: ((URL, [String:Any], (Result<URL,Error>)->Void))?
    private let inFlight=DispatchSemaphore(value:3)
    private var drawableFrames=0

    init(view: MTKView) throws {
        guard let device=MTLCreateSystemDefaultDevice(),let queue=device.makeCommandQueue() else { throw RuntimeError.message("Metal is unavailable") }
        self.device=device;self.queue=queue;self.view=view
        let stage=Bundle.main.object(forInfoDictionaryKey:"SanctuaryMode") as? String == "soundstage"
        isSoundstage=stage;world=try GardenWorld(soundstageOnly:stage)
        super.init()
        view.device=device;view.colorPixelFormat = .bgra8Unorm_srgb
        view.sampleCount=4
        view.depthStencilPixelFormat = .depth32Float;view.framebufferOnly=false
        view.autoResizeDrawable=false;view.drawableSize=CGSize(width:1920,height:1080)
        view.preferredFramesPerSecond=60
        view.clearColor=MTLClearColor(red:0.4,green:0.6,blue:0.65,alpha:1)
        try buildPipelines()
        windBuffers=(0..<3).map{_ in device.makeBuffer(length:1024*16,options:.storageModeShared)!}
        batches=world.batches.map(upload)
        pod=upload(SceneBatch(name:"Seed pod",mesh:world.studioMesh,instances:[Instance()]))
        let floorShape=Shape.box(V3(2,0.01,2))
        floor=upload(SceneBatch(name:"Studio floor",mesh:try Mesher.compile(floorShape,resolution:8),instances:[Instance(position:V3(0,-0.01,0),tint:V3(0.36,0.38,0.34),kind:7)]))
        resetCamera()
        try applySource(AssetSource.load(isSoundstage ? "tree":"seed"))
        scene=isSoundstage ? "studio":"garden"
        resetCamera()
        if isSoundstage {paused=true}
        view.delegate=self
    }

    func upload(_ batch: SceneBatch) -> GPUBatch {
        precondition(batch.instances.allSatisfy {$0.tint.w == batch.instances.first?.tint.w},"Each draw batch must have one material")
        let vertices=device.makeBuffer(bytes:batch.mesh.vertices,length:MemoryLayout<Vertex>.stride*batch.mesh.vertices.count)!
        let indices=device.makeBuffer(bytes:batch.mesh.indices,length:MemoryLayout<UInt32>.stride*batch.mesh.indices.count)!
        let instances=device.makeBuffer(bytes:batch.instances,length:MemoryLayout<Instance>.stride*batch.instances.count)!
        vertices.label=batch.name+" vertices";indices.label=batch.name+" indices"
        let points=batch.mesh.vertices.map{V3($0.position.x,$0.position.y,$0.position.z)}
        let lower=points.reduce(V3(repeating:Float.greatestFiniteMagnitude),simd_min)
        let upper=points.reduce(V3(repeating:-Float.greatestFiniteMagnitude),simd_max)
        let center=(lower+upper)/2,radius=length(upper-lower)/2+1.0
        let buffers=(0..<6).map{_ in device.makeBuffer(length:MemoryLayout<Instance>.stride*max(1,batch.instances.count),options:.storageModeShared)!}
        let lods=batch.lodMeshes.map {mesh -> GPUBatch in var lod=SceneBatch(name:batch.name,mesh:mesh,instances:batch.instances);lod.roughness=batch.roughness;lod.metallic=batch.metallic;return upload(lod)}
        let clusters=MeshletData(batch.mesh)
        struct Cluster {var ranges:SIMD4<UInt32>;var sphere:SIMD4<Float>}
        let descriptors=zip(clusters.descriptors,clusters.spheres).map{Cluster(ranges:$0.0,sphere:$0.1)}
        return GPUBatch(name:batch.name,vertices:vertices,indices:indices,instances:instances,indexCount:batch.mesh.indices.count,instanceCount:batch.instances.count,sourceInstances:batch.instances,visibleBuffers:buffers,center:center,radius:radius,lods:lods,error:batch.mesh.geometricError,meshlets:device.makeBuffer(bytes:descriptors,length:descriptors.count*32)!,meshletVertices:device.makeBuffer(bytes:clusters.vertices,length:clusters.vertices.count*4)!,meshletTriangles:device.makeBuffer(bytes:clusters.triangles,length:clusters.triangles.count)!,material:SIMD4(batch.roughness,batch.metallic,0,0),meshletCount:descriptors.count)
    }

    func buildPipelines(sourceURL: URL? = nil) throws {
        var source=try String(contentsOf:sourceURL ?? Bundle.main.url(forResource:"Garden",withExtension:"metal") ?? Bundle.module.url(forResource:"Garden",withExtension:"metal")!,encoding:.utf8)
        let atmosphereURL=sourceURL?.deletingLastPathComponent().appendingPathComponent("Atmosphere.metal") ?? Bundle.main.url(forResource:"Atmosphere",withExtension:"metal") ?? Bundle.module.url(forResource:"Atmosphere",withExtension:"metal")!
        source=source.replacingOccurrences(of:"// ATMOSPHERE",with:try String(contentsOf:atmosphereURL,encoding:.utf8))
        let library=try device.makeLibrary(source:source,options:nil)
        func fragment(_ kind:Int,_ name:String="gardenFragment") throws ->MTLFunction {
            let constants=MTLFunctionConstantValues();var k=Int32(kind)
            constants.setConstantValue(&k,type:.int,index:0)
            return try library.makeFunction(name:name,constantValues:constants)
        }
        let genericFragment=try fragment(-1)
        let d=MTLRenderPipelineDescriptor()
        d.vertexFunction=library.makeFunction(name:"gardenVertex");d.fragmentFunction=genericFragment
        d.rasterSampleCount=view.sampleCount;d.colorAttachments[0].pixelFormat = .rgba16Float;d.depthAttachmentPixelFormat = .depth32Float
        let newPipeline=try device.makeRenderPipelineState(descriptor:d)
        d.vertexFunction=library.makeFunction(name:"skyVertex");d.fragmentFunction=library.makeFunction(name:"skyFragment")
        let newSkyPipeline=try device.makeRenderPipelineState(descriptor:d)
        d.fragmentFunction=try fragment(7,"groundFragment")
        let newGroundPipeline=try device.makeRenderPipelineState(descriptor:d)
        let sd=MTLRenderPipelineDescriptor();sd.vertexFunction=library.makeFunction(name:"shadowVertex");sd.depthAttachmentPixelFormat = .depth32Float
        let newShadowPipeline=try device.makeRenderPipelineState(descriptor:sd)
        let md=MTLMeshRenderPipelineDescriptor();md.meshFunction=library.makeFunction(name:"gardenMesh");md.fragmentFunction=genericFragment
        md.maxTotalThreadsPerMeshThreadgroup=64;md.rasterSampleCount=view.sampleCount;md.colorAttachments[0].pixelFormat = .rgba16Float;md.depthAttachmentPixelFormat = .depth32Float
        let (newMeshPipeline,_)=try device.makeRenderPipelineState(descriptor:md,options:[])
        var specialized:[Int:MTLRenderPipelineState]=[:],specializedMesh:[Int:MTLRenderPipelineState]=[:]
        d.vertexFunction=library.makeFunction(name:"gardenVertex")
        for kind in 0...8 {
            let f=try fragment(kind);d.fragmentFunction=f;md.fragmentFunction=f
            specialized[kind]=try device.makeRenderPipelineState(descriptor:d)
            specializedMesh[kind]=try device.makeRenderPipelineState(descriptor:md,options:[]).0
        }
        let newAtmosphere=try Atmosphere(device:device,library:library)
        let newPost=try PostProcess(device:device,library:library)
        materialPipelines=specialized;materialMeshPipelines=specializedMesh
        post=newPost
        atmosphere=newAtmosphere;meshPipeline=newMeshPipeline
        pipeline=newPipeline;skyPipeline=newSkyPipeline;groundPipeline=newGroundPipeline;shadowPipeline=newShadowPipeline
        shaderDigest=SHA256.hash(data:Data(source.utf8)).map{String(format:"%02x",$0)}.joined()
        let depth=MTLDepthStencilDescriptor();depth.depthCompareFunction = .less;depth.isDepthWriteEnabled=true
        depthState=device.makeDepthStencilState(descriptor:depth)
        depth.depthCompareFunction = .lessEqual;depth.isDepthWriteEnabled=false
        skyDepthState=device.makeDepthStencilState(descriptor:depth)
        let texture=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.depth32Float,width:2048,height:2048,mipmapped:false)
        texture.usage=[.renderTarget,.shaderRead];texture.storageMode = .private
        shadowTexture=device.makeTexture(descriptor:texture)
    }

    func resetCamera() {
        studioViewName="Custom";camera=V3(0,world.terrain.height(0,24)+1.72,24);yaw=0;pitch = -0.04
        orbit=0.48;orbitDistance=studioLayout.arrangement=="grove" ? 13:6.8;orbitElevation=0.18;keys.removeAll()
        if scene=="studio" {fitStudioCamera()}
    }
    var forward:V3 { V3(sin(yaw)*cos(pitch),sin(pitch),-cos(yaw)*cos(pitch)) }
    var podPosition:V3 { V3(5,world.terrain.height(5,12)-world.studioShape.bounds.min.y*podScale*GardenWorld.podUnitScale,12) }

    func look(dx:Float,dy:Float) {
        if scene=="studio" {studioViewName="Custom"; orbit+=dx*0.006;orbitElevation=clamp(orbitElevation+dy*0.004,-0.1,1.5) }
        else { yaw+=dx*0.003;pitch=clamp(pitch-dy*0.003,-1.35,1.35) }
    }

    func move(local:V3) {
        guard scene=="garden" else { return }
        let right=V3(cos(yaw),0,sin(yaw)), forward=V3(sin(yaw),0,-cos(yaw))
        let delta=right*local.x+forward*local.z
        let count=max(1,Int(ceil(length(delta)/0.15)))
        for _ in 0..<count {
            var p=camera+delta/Float(count)
            p.x=clamp(p.x,-130,130);p.z=clamp(p.z,-135,120)
            var ground=world.terrain.height(p.x,p.z)
            for off in [V3(0.25,0,0),V3(-0.25,0,0),V3(0,0,0.25),V3(0,0,-0.25)] {
                ground=max(ground,world.terrain.height(p.x+off.x,p.z+off.z))
            }
            p.y=ground+1.72
            var nearby=world.solids.filter { length_squared(V3(p.x-$0.position.x,0,p.z-$0.position.z)) < pow(10*$0.scale,2) }
            nearby.append(Solid(shape:world.studioShape,position:podPosition,scale:podScale*GardenWorld.podUnitScale,name:"Seed pod"))
            for solid in nearby { for height:Float in [0.35,1.0,1.55] {
                let sample=V3(p.x,ground+height,p.z), d=solid.value(sample)
                if d<0.28 {
                    let e:Float=0.01
                    var n=V3(solid.value(sample+V3(e,0,0))-solid.value(sample-V3(e,0,0)),0,solid.value(sample+V3(0,0,e))-solid.value(sample-V3(0,0,e)))
                    if length_squared(n)>0.000001 { n=normalize(n);p+=n*(0.28-d) }
                }
            }}
            p.y=max(world.terrain.height(p.x,p.z),ground)+1.72
            camera=p
        }
    }

    func step(_ dt: Float) {
        simulationTime+=dt
        if scene=="studio" && studioLayout.turntable {orbit+=dt*0.25}
        atmosphereWind.step(dt)
        if scene=="garden" {
            var m=V3.zero
            if keys.contains(13) { m.z+=1 };if keys.contains(1) { m.z-=1 }
            if keys.contains(0) { m.x-=1 };if keys.contains(2) { m.x+=1 }
            if length_squared(m)>0 { move(local:normalize(m)*dt*(keys.contains(56) ? 10 : 5.5)) }
        }
    }

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}
    func draw(in view: MTKView) {
        let start=CACurrentMediaTime(), elapsed=start-lastTime;lastTime=start
        if measuredScene != scene {clearMetrics();measuredScene=scene}
        if frame>3 { frameTimes.append(elapsed*1000);if frameTimes.count>3600 {frameTimes.removeFirst()} }
        if !paused {
            accumulator+=min(elapsed,0.05)
            while accumulator >= 1.0/60 {step(1/60);accumulator-=1.0/60}
        } else {accumulator=0}
        guard inFlight.wait(timeout:.now()) == .success else { return }
        let drawableStart=CACurrentMediaTime()
        guard let drawable=view.currentDrawable,let pass=view.currentRenderPassDescriptor,let cb=queue.makeCommandBuffer() else {inFlight.signal();return}
        let encodeStart=CACurrentMediaTime()
        drawableWaitTimes.append((encodeStart-drawableStart)*1000);if drawableWaitTimes.count>3600 {drawableWaitTimes.removeFirst()}
        drawableFrames+=1
        let sun=indoorStudio ? normalize(rigPosition-studioAssetCenter):skySettings.sunDirection
        var eye:V3, target:V3
        if scene=="studio" {
            target=studioCenter
            eye=target+V3(sin(orbit)*cos(orbitElevation),sin(orbitElevation),cos(orbit)*cos(orbitElevation))*orbitDistance*studioUnit
        } else {eye=camera;target=camera+forward}
        if scene=="studio" && studioObject=="sky" {
            eye=V3(0,1.72,0);target=eye+V3(sin(orbit)*cos(orbitElevation),sin(orbitElevation),-cos(orbit)*cos(orbitElevation))
        }
        let vp=perspective(1.05,16/9,scene=="studio" ? max(0.00001,studioExtent*0.001):0.08,450)*lookAt(eye,target)
        // A millimetre-scale object camera must not determine the precision of
        // rays aimed at the sun. Reconstruct sky directions without translation.
        let skyInverse=(perspective(1.05,16/9,0.1,450)*lookAt(.zero,target-eye)).inverse
        let lightCenter=scene=="studio" ? studioAssetCenter:V3(camera.x,0,camera.z-20)
        let lightSize:Float=scene=="studio" ? max(0.02,studioExtent*(studioLayout.arrangement=="grove" ? 3:1.2)):65
        let lamp=indoorStudio ? rigPosition:lightCenter+sun*(scene=="studio" ? studioExtent*5:110)
        let lightView=lookAt(lamp,lightCenter,up:abs(sun.y)>0.98 ? V3(0,0,1):V3(0,1,0))
        var lvp=(indoorStudio ? perspective(1.9,1,max(0.00001,studioExtent*0.01),studioExtent*12):orthographic(lightSize,scene=="studio" ? max(0.00001,studioExtent*0.01):1,scene=="studio" ? studioExtent*12:250))*lightView
        // Anchor the shadow projection to whole texels instead of following every camera subpixel.
        let origin=lvp*SIMD4<Float>(0,0,0,1)
        lvp[3].x += (round(origin.x*1024)-origin.x*1024)/1024
        lvp[3].y += (round(origin.y*1024)-origin.y*1024)/1024
        var uniforms=Uniforms(viewProjection:vp,lightVP:lvp,inverseVP:skyInverse,cameraTime:SIMD4(eye,simulationTime),sunExposure:SIMD4(sun,exposure),options:SIMD4(lighting=="sunset" ? 1:0,wind,scene=="studio" ? 1:0,0),sky:SIMD4(skySettings.coverage,skySettings.density,skySettings.haze,skySettings.cloudSeed),environment:SIMD4(["morning":0,"noon":0,"sunset":1,"overcast":2,"rain":3,"indoor":4,"golden":5,"afterglow":6][lighting] ?? 0,wetness,0,0))
        if indoorStudio {uniforms.environment.x=4}
        uniforms.rigPosition=SIMD4(rigPosition,indoorStudio ? 1:0)
        let warmth=studioRig.warmth
        uniforms.rigColor=SIMD4(V3(1,1-warmth*0.25,1-warmth*0.55),studioRig.intensity*studioExtent*studioExtent)
        uniforms.rigParams=SIMD4(studioRig.size,studioRig.fill,studioExtent,0)
        uniforms.lookSurface=sceneLook.surface;uniforms.lookLight=sceneLook.light;uniforms.lookGrade=sceneLook.grade
        let profile=profiling && frame%5==0 ? GPUProfile(device):nil
        atmosphere.encode(cb,&uniforms,paused:paused,profile:profile)
        let lightState=post.encodeLighting(cb,uniforms:&uniforms,atmosphere:atmosphere,index:frame%3,profile:profile)
        shadowTexelsPerMetre=1024/lightSize
        let cells=atmosphereWind.gpuCells
        let windBuffer=windBuffers[frame%3]
        _ = cells.withUnsafeBytes {memcpy(windBuffer.contents(),$0.baseAddress!,$0.count)}
        let shadow=MTLRenderPassDescriptor()
        shadow.depthAttachment.texture=shadowTexture;shadow.depthAttachment.loadAction = .clear;shadow.depthAttachment.storeAction = .store;shadow.depthAttachment.clearDepth=1
        shadowTriangles=0
        if !(scene=="studio" && studioObject=="sky") {
        profile?.render(shadow,"shadow")
        if let enc=cb.makeRenderCommandEncoder(descriptor:shadow) {
            enc.label="Sun shadows";enc.setRenderPipelineState(shadowPipeline);enc.setDepthStencilState(depthState)
            enc.setDepthBias(1,slopeScale:1,clamp:0.001)
            enc.setVertexBytes(&uniforms,length:MemoryLayout<Uniforms>.stride,index:1)
            enc.setVertexBuffer(windBuffer,offset:0,index:3)
            cullMatrix=lvp;shadowTriangles=0;renderObjects(enc,shadow:true);enc.endEncoding()
        }
        }
        pass.colorAttachments[0].texture=post.multisample;pass.colorAttachments[0].resolveTexture=post.hdr;pass.colorAttachments[0].storeAction = .multisampleResolve
        profile?.render(pass,"scene")
        if let enc=cb.makeRenderCommandEncoder(descriptor:pass) {
            enc.label="Field garden";enc.setVertexBytes(&uniforms,length:MemoryLayout<Uniforms>.stride,index:1)
            enc.setFragmentBytes(&uniforms,length:MemoryLayout<Uniforms>.stride,index:1)
            enc.setFragmentBuffer(lightState,offset:0,index:2)
            enc.setFragmentTexture(shadowTexture,index:0);enc.setFragmentTexture(atmosphere.sky,index:1);enc.setFragmentTexture(atmosphere.irradiance,index:2);enc.setFragmentTexture(atmosphere.trans,index:3)
            enc.setFragmentTexture(atmosphere.previousSky,index:4);enc.setFragmentTexture(atmosphere.previousIrradiance,index:5);enc.setFragmentTexture(atmosphere.clearSky,index:6)
            enc.setVertexBuffer(windBuffer,offset:0,index:3)
            enc.setMeshBytes(&uniforms,length:MemoryLayout<Uniforms>.stride,index:1);enc.setMeshBuffer(windBuffer,offset:0,index:3)
            enc.setRenderPipelineState(useMeshlets ? meshPipeline:pipeline);enc.setDepthStencilState(depthState)
            cullMatrix=vp;visibleTriangles=0;renderObjects(enc,shadow:false)
            if scene=="studio" && studioObject != "sky" && studioRig.name != "room" && studioLayout.ground {
                var material=SIMD4<Float>(-1,0,0,2)
                enc.setFragmentBytes(&material,length:MemoryLayout<SIMD4<Float>>.stride,index:4)
                enc.setRenderPipelineState(groundPipeline);enc.setDepthStencilState(depthState);enc.setCullMode(.none)
                enc.drawPrimitives(type:.triangle,vertexStart:0,vertexCount:3)
            }
            enc.setRenderPipelineState(skyPipeline);enc.setDepthStencilState(skyDepthState);enc.setCullMode(.none)
            enc.drawPrimitives(type:.triangle,vertexStart:0,vertexCount:3);enc.endEncoding()
        }
        post.encode(cb,to:drawable.texture,lightState:lightState,look:sceneLook.grade,profile:profile)
        var pending:(URL,[String:Any],(Result<URL,Error>)->Void,MTLBuffer,Int)?
        if let request=captureRequest {
            captureRequest=nil
            let row=1920*4
            if let buffer=device.makeBuffer(length:row*1080,options:.storageModeShared),let blit=cb.makeBlitCommandEncoder() {
                blit.copy(from:drawable.texture,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(x:0,y:0,z:0),sourceSize:MTLSize(width:1920,height:1080,depth:1),to:buffer,destinationOffset:0,destinationBytesPerRow:row,destinationBytesPerImage:row*1080)
                var metadata=snapshot();metadata["frame"]=frame+1;metadata["captureIncludesHUD"]=false
                let sunClip=vp*SIMD4(eye+sun*100,1)
                if sunClip.w>0 {metadata["projectedSunPixels"]=[(sunClip.x/sunClip.w*0.5+0.5)*1920,(0.5-sunClip.y/sunClip.w*0.5)*1080]}
                blit.endEncoding();pending=(request.0,metadata,request.2,buffer,row)
            } else { request.2(.failure(RuntimeError.message("Unable to allocate capture buffer"))) }
        }
        cb.present(drawable)
        let semaphore=inFlight
        let capture=pending,submittedScene=scene,submittedPhase=atmosphere.updatePhase,submittedGeneration=metricsGeneration
        cb.addCompletedHandler { [weak self] command in
            semaphore.signal()
            let ms=(command.gpuEndTime-command.gpuStartTime)*1000
            let passes=profile?.resolve() ?? [:]
            DispatchQueue.main.async {
                guard let self else { return }
                if let error=command.error {self.gpuErrors.append(error.localizedDescription)}
                if ms>0 && self.scene==submittedScene && self.metricsGeneration==submittedGeneration {
                    for (name,duration) in passes {
                        self.gpuPassTimes[name,default:[]].append(duration)
                        if self.gpuPassTimes[name]!.count>512 {self.gpuPassTimes[name]!.removeFirst()}
                    }
                    self.gpuTimes.append(ms);if self.gpuTimes.count>3600 {self.gpuTimes.removeFirst()}
                    self.gpuPhaseTimes[submittedPhase,default:[]].append(ms)
                    if self.gpuPhaseTimes[submittedPhase]!.count>512 {self.gpuPhaseTimes[submittedPhase]!.removeFirst()}
                }
                if let p=capture {
                    if let error=command.error { p.2(.failure(error)) }
                    else { self.writeCapture(p.0,p.1,p.2,p.3,p.4) }
                }
            }
        }
        cb.commit();frame+=1
        cpuTimes.append((CACurrentMediaTime()-encodeStart)*1000);if cpuTimes.count>3600 {cpuTimes.removeFirst()}
        if frame%20==0 {onUpdate?()}
    }

    func renderObjects(_ enc: MTLRenderCommandEncoder, shadow: Bool) {
        if scene=="studio" && studioObject=="sky" {return}
        let rows=cullMatrix.transpose
        let planes=[rows[3]+rows[0],rows[3]-rows[0],rows[3]+rows[1],rows[3]-rows[1],rows[2],rows[3]-rows[2]]
        enc.setFrontFacing(.counterClockwise)
        func draw(_ b:GPUBatch,instance:Instance? = nil,level:Int = 0,usesLOD:Bool = false,subject:Bool=false,emitter:Bool=false) {
            if !b.lods.isEmpty && instance == nil {
                draw(b.lods[0],level:1,usesLOD:true);draw(b.lods[1],level:2,usesLOD:true)
            }
            if !shadow {
                var material=b.material
                if subject {
                    if studioLayout.roughness>=0 {material.x=studioLayout.roughness}
                    if studioLayout.metallic>=0 {material.y=studioLayout.metallic}
                }
                if emitter {material.z=1}
                if b.name=="Studio floor" && !emitter {material.w=1}
                enc.setFragmentBytes(&material,length:MemoryLayout<SIMD4<Float>>.stride,index:4)
                let kind=Int(instance?.tint.w ?? b.sourceInstances.first?.tint.w ?? 0)
                enc.setRenderPipelineState(useMeshlets ? materialMeshPipelines[kind] ?? meshPipeline:materialPipelines[kind] ?? pipeline)
            }
            var count=1
            enc.setVertexBuffer(b.vertices,offset:0,index:0)
            if !shadow && useMeshlets {enc.setMeshBuffer(b.vertices,offset:0,index:0)}
            if var instance { enc.setVertexBytes(&instance,length:MemoryLayout<Instance>.stride,index:2);if !shadow && useMeshlets {enc.setMeshBytes(&instance,length:MemoryLayout<Instance>.stride,index:2)} }
            else {
                let parent=batches.first(where:{$0.name==b.name})
                let visible=b.sourceInstances.enumerated().filter { index,inst in
                    let center=inst.model*SIMD4(b.center,1)
                    let scale=max(length(inst.model[0]),max(length(inst.model[1]),length(inst.model[2])))
                    if usesLOD || !b.lods.isEmpty,let parent {
                        let distance=max(1,length(V3(center.x,center.y,center.z)-camera)-b.radius*scale)
                        let errors=[Float(0)]+parent.lods.map(\.error)
                        let current=lodSelection[b.name]?[index] ?? 0
                        var selected=0
                        if shadow {
                            // Select against shadow texels, not the eye's projection.
                            // Keep simplification below one texel of the 5-texel filter.
                            for k in 1..<errors.count {if errors[k]*scale*shadowTexelsPerMetre<0.8 {selected=k}}
                        } else {
                            for k in 1..<errors.count {if errors[k]*scale*935/distance < (k==current ? 0.65:0.4) {selected=k}}
                            lodSelection[b.name,default:[:]][index]=selected
                        }
                        if selected != level {return false}
                    }
                    return planes.allSatisfy { dot($0,center) >= -b.radius*scale*length(V3($0.x,$0.y,$0.z)) }
                }.map(\.element)
                count=visible.count;guard count>0 else {return}
                let buffer=b.visibleBuffers[(frame%3)*2+(shadow ? 1:0)]
                _ = visible.withUnsafeBytes { data in memcpy(buffer.contents(),data.baseAddress!,data.count) }
                enc.setVertexBuffer(buffer,offset:0,index:2)
                if !shadow && useMeshlets {enc.setMeshBuffer(buffer,offset:0,index:2)}
            }
            enc.setCullMode(b.name=="Meadow" || b.name=="Wildflowers" || b.name=="Leaves" ? .none:.back)
            if !shadow && useMeshlets {
                enc.setMeshBuffer(b.meshlets,offset:0,index:4);enc.setMeshBuffer(b.meshletVertices,offset:0,index:5);enc.setMeshBuffer(b.meshletTriangles,offset:0,index:6)
                enc.drawMeshThreadgroups(MTLSize(width:b.meshletCount,height:count,depth:1),threadsPerObjectThreadgroup:MTLSize(width:1,height:1,depth:1),threadsPerMeshThreadgroup:MTLSize(width:64,height:1,depth:1))
            } else {enc.drawIndexedPrimitives(type:.triangle,indexCount:b.indexCount,indexType:.uint32,indexBuffer:b.indices,indexBufferOffset:0,instanceCount:count)}
            if shadow {shadowTriangles+=b.indexCount/3*count} else {visibleTriangles+=b.indexCount/3*count}
        }
        if scene=="garden" {
            for b in batches { if shadow && (b.name=="Meadow" || b.name=="Wildflowers" || b.name=="Leaves") {continue};draw(b) }
            draw(pod,instance:Instance(position:podPosition,scale:V3(repeating:podScale*GardenWorld.podUnitScale),tint:V3(0.48,0.27,0.13),kind:6))
        } else {
            let e=studioExtent
            if studioRig.name=="room" {
                draw(floor,instance:Instance(position:V3(0,-e*0.01,0),scale:V3(e*4,e,e*4),tint:V3(repeating:0.43),kind:7))
            }
            if studioRig.name=="room" {
                for z:Float in [-4,4] {draw(floor,instance:Instance(position:V3(0,e*1.5,z*e),scale:V3(e*2,e*150,e*0.02),tint:V3(0.55,0.53,0.5),kind:7))}
                for x:Float in [-4,4] {draw(floor,instance:Instance(position:V3(x*e,e*1.5,0),scale:V3(e*0.02,e*150,e*2),tint:V3(0.5,0.53,0.55),kind:7))}
                draw(floor,instance:Instance(position:V3(0,e*3,0),scale:V3(e*2,e,e*2),tint:V3(repeating:0.6),kind:7))
            }
            if indoorStudio && !shadow {
                draw(floor,instance:Instance(position:rigPosition,scale:V3(e*studioRig.size,e*studioRig.size,e*studioRig.size),tint:V3(1,1-studioRig.warmth*0.25,1-studioRig.warmth*0.55),kind:7),emitter:true)
            }
            let center=(studioBounds.min+studioBounds.max)/2
            let offsets:[V3]=studioLayout.arrangement=="grove" ? [.zero,V3(-1.25*e,0,-e),V3(1.25*e,0,-e)]:[.zero]
            for (i,offset) in offsets.enumerated() {
                for batch in studioBatches {
                    let tint=batch.sourceInstances[0].tint
                    var instance=Instance(position:offset+V3(-center.x,-studioBounds.min.y,-center.z)*studioLayout.scale,scale:V3(repeating:studioLayout.scale),yaw:Float(i)*0.7,tint:V3(tint.x,tint.y,tint.z)*studioLayout.tint,kind:tint.w)
                    instance.model=instance.model*batch.sourceInstances[0].model
                    draw(batch,instance:instance,subject:true)
                }
            }
            if studioLayout.reference {
                // A 1.72 m measuring post, with a short horizontal eye-height bar.
                draw(floor,instance:Instance(position:V3(e*0.7,0.86,0),scale:V3(0.015,86,0.015),tint:V3(0.8,0.38,0.14),kind:7))
                draw(floor,instance:Instance(position:V3(e*0.7,1.72,0),scale:V3(0.12,1,0.015),tint:V3(0.8,0.38,0.14),kind:7))
            }
        }
    }

    func writeCapture(_ url:URL,_ metadata:[String:Any],_ completion:(Result<URL,Error>)->Void,_ buffer:MTLBuffer,_ row:Int) {
        do {
            let bytes=buffer.contents().bindMemory(to:UInt8.self,capacity:row*1080)
            guard let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:1920,pixelsHigh:1080,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:row,bitsPerPixel:32) else {throw RuntimeError.message("Capture bitmap allocation failed")}
            let out=bitmap.bitmapData!
            for i in stride(from:0,to:row*1080,by:4) {out[i]=bytes[i+2];out[i+1]=bytes[i+1];out[i+2]=bytes[i];out[i+3]=255}
            guard let png=bitmap.representation(using:.png,properties:[:]) else {throw RuntimeError.message("PNG encoding failed")}
            try png.write(to:url,options:.atomic)
            try JSONSerialization.data(withJSONObject:metadata,options:[.prettyPrinted,.sortedKeys]).write(to:url.deletingPathExtension().appendingPathExtension("json"),options:.atomic)
            completion(.success(url))
        } catch {completion(.failure(error))}
    }

    func stats(_ values:[Double]) -> [String:Double] {
        guard !values.isEmpty else {return [:]}
        let sorted=values.sorted()
        func percentile(_ p:Double)->Double {sorted[min(sorted.count-1,Int(Double(sorted.count-1)*p))]}
        return ["median":percentile(0.5),"p95":percentile(0.95),"p99":percentile(0.99),"max":sorted.last!]
    }
    func clearMetrics() {metricsGeneration+=1;gpuPassTimes=[:];gpuPhaseTimes=[:];gpuTimes=[];cpuTimes=[];frameTimes=[];drawableWaitTimes=[]}
    func snapshot() -> [String:Any] {
        let triangles=scene=="garden" ? batches.reduce(0){$0+$1.indexCount/3*$1.instanceCount}+pod.indexCount/3 : visibleTriangles
        return ["sceneLook":sceneLook.values,"gameTreeParameters":world.treeSource.parameters,"workshop":workshopSnapshot(),"mode":isSoundstage ? "soundstage":"game","sky":skySettings.snapshot,"scene":scene,"studioObject":studioObject,"antialiasing":"4x MSAA","seed":world.seed,"frame":frame,"time":simulationTime,"paused":paused,
                "camera":["position":[camera.x,camera.y,camera.z],"yaw":yaw,"pitch":pitch,"orbit":orbit,"elevation":orbitElevation,"distance":orbitDistance],
                "lighting":lighting,"parameters":["exposure":exposure,"wind":wind,"scale":podScale],
                "renderSize":[1920,1080],"device":device.name,"fullDetailTriangles":triangles,
                "thermalState":ProcessInfo.processInfo.thermalState.rawValue,"lowPowerMode":ProcessInfo.processInfo.isLowPowerModeEnabled,"skySnapshotTimes":[atmosphere.previousPublishedTime,atmosphere.publishedTime],"gpuPassSampleCounts":gpuPassTimes.mapValues(\.count),"profiling":profiling,"gpuCountersSupported":GPUProfile.supported(device),"gpuPassMilliseconds":gpuPassTimes.mapValues {stats($0)},"gpuMilliseconds":stats(gpuTimes),"frameGPUBySkyPhase":gpuPhaseTimes.mapValues {stats($0)},"skyUpdatePhase":atmosphere.updatePhase,"weather":["model":"moist columns","seconds":atmosphere.weatherTime,"totalWater":atmosphere.weather.totalWater,"evaporated":atmosphere.weather.evaporated,"precipitated":atmosphere.weather.precipitated],"cpuEncodeMilliseconds":stats(cpuTimes),"frameIntervalMilliseconds":stats(frameTimes),
                "drawableWaitMilliseconds":stats(drawableWaitTimes),"visibleTriangles":visibleTriangles,"shadowTriangles":shadowTriangles,
                "timingSampleCount":gpuTimes.count,"allocatedMetalBytes":device.currentAllocatedSize,
                "groundHeight":world.terrain.height(camera.x,camera.z),"sourceRevision":ProcessInfo.processInfo.environment["SANCTUARY_REVISION"] ?? Bundle.main.object(forInfoDictionaryKey:"SanctuaryRevision") as? String ?? "working-tree",
                "sourceDigest":Bundle.main.object(forInfoDictionaryKey:"SanctuarySourceDigest") as? String ?? "unbundled","shaderDigest":shaderDigest,"gpuErrors":gpuErrors,
                "renderer":useMeshlets ? "Field-derived meshes / Metal mesh shaders":"Field-derived meshes / indexed raster","worldUnit":"metre","seedPodHeightMetres":GardenWorld.podMetres*podScale,"wetness":wetness,"indirectLighting":"sky irradiance with approximate ground bounce","exposureMeter":"clear sky hemisphere, gain 1...24","skyCacheSize":[atmosphere.sky.width,atmosphere.sky.height],"cloudModel":"conservative moist-air transport, saturation adjustment, buoyancy and procedural 3D density","cloudCacheExtentKm":64,"windTicks":atmosphereWind.ticks,"sessionPID":ProcessInfo.processInfo.processIdentifier]
    }
}

enum RuntimeError:LocalizedError {
    case message(String)
    var errorDescription:String? {if case let .message(s)=self {return s};return nil}
}
