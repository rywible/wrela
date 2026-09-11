import Foundation
import simd
import FieldCore
import FieldCompiler

struct Instance {
    var model: simd_float4x4
    var tint: SIMD4<Float>
    init(position: V3 = .zero, scale: V3 = V3(repeating:1), yaw: Float = 0, tint: V3 = V3(repeating:1), kind: Float = 0) {
        model = transform(position,scale,yaw); self.tint = SIMD4(tint,kind)
    }
}

func transform(_ p: V3, _ s: V3, _ yaw: Float) -> simd_float4x4 {
    let c=cos(yaw), t=sin(yaw)
    return simd_float4x4(columns:(SIMD4(c*s.x,0,-t*s.x,0),SIMD4(0,s.y,0,0),SIMD4(t*s.z,0,c*s.z,0),SIMD4(p,1)))
}

struct SceneBatch {
    var name: String
    var mesh: Mesh
    var instances: [Instance]
    var lodMeshes:[Mesh] = []
    var roughness:Float = -1
    var metallic:Float = 0
}

struct Solid {
    var shape: Shape
    var position: V3
    var scale: Float
    var name: String
    func value(_ p: V3) -> Float { shape.value(at:(p-position)/scale)*scale }
}

struct GardenWorld {
    static let podMetres: Float = 0.035
    static var podUnitScale: Float { podMetres / (podShape.bounds.max.y-podShape.bounds.min.y) }
    let terrain = Terrain()
    var batches: [SceneBatch] = []
    var solids: [Solid] = []
    var studioMesh: Mesh
    var treeSource=AssetSource.defaults("tree")
    var studioShape: Shape
    let seed: UInt64 = 82317

    init(soundstageOnly:Bool=false) throws {
        func mesh(_ s: Shape, _ r: Int) throws -> Mesh { try Mesher.compile(s,resolution:r) }
        // A sculptural seed pod exercises smooth composition and subtraction.
        studioShape = Self.podShape
        studioMesh = try mesh(studioShape,58)
        if !soundstageOnly {batches.append(SceneBatch(name:"Terrain",mesh:Mesher.terrain(terrain),instances:[Instance(kind:1)]))}

        treeSource=try AssetSource.load("tree")
        let (branches,crown)=treeSource.treeFields()
        let trunk=branches
        let trunkMesh=try mesh(branches,48)
        let crownMesh=try mesh(crown,48)
        let rockShape=AssetSource.stoneField
        let rockMesh=try mesh(rockShape,40)
        if soundstageOnly {
            batches=[SceneBatch(name:"Trunks",mesh:trunkMesh,instances:[Instance(kind:4)]),SceneBatch(name:"Stones",mesh:rockMesh,instances:[Instance(kind:5)])]
            return
        }
        var trunks:[Instance]=[], crowns:[Instance]=[], rocks:[Instance]=[]
        var rng=SeededRandom(seed:seed)
        for _ in 0..<260 {
            let x=rng.range(-105,105), z=rng.range(-125,65)
            let pathDistance=abs(x-terrain.pathX(z))
            if pathDistance < 5 || (abs(x)<13 && z > 9 && z < 33) { continue }
            let s=rng.range(1.1,2.3), p=V3(x,terrain.height(x,z)-0.15,z), yaw=rng.range(0,6.28)
            trunks.append(Instance(position:p,scale:V3(repeating:s),yaw:yaw,tint:V3(0.31,0.25,0.18),kind:4))
            let gold=rng.next()
            let color = gold < 0.22 ? V3(0.73,0.70,0.32) : V3(rng.range(0.33,0.48),rng.range(0.57,0.70),rng.range(0.27,0.39))
            crowns.append(Instance(position:p,scale:V3(repeating:s),yaw:yaw,tint:color,kind:2))
            solids.append(Solid(shape:trunk,position:p,scale:s,name:"Tree"))
        }
        for _ in 0..<95 {
            let x=rng.range(-75,75), z=rng.range(-105,50)
            if abs(x-terrain.pathX(z))<3.4 { continue }
            let s=rng.range(0.5,2.3), p=V3(x,terrain.height(x,z),z)
            rocks.append(Instance(position:p,scale:V3(repeating:s),tint:V3(0.53,0.57,0.51),kind:5))
            solids.append(Solid(shape:rockShape,position:p,scale:s,name:"Stone"))
        }
        batches += [SceneBatch(name:"Trunks",mesh:trunkMesh,instances:trunks,lodMeshes:[MeshProcessing.simplify(trunkMesh,ratio:0.45,error:0.012),MeshProcessing.simplify(trunkMesh,ratio:0.18,error:0.035)]),SceneBatch(name:"Canopies",mesh:crownMesh,instances:crowns,lodMeshes:[MeshProcessing.simplify(crownMesh,ratio:0.35,error:0.03),MeshProcessing.simplify(crownMesh,ratio:0.12,error:0.08)]),SceneBatch(name:"Stones",mesh:rockMesh,instances:rocks,lodMeshes:[MeshProcessing.simplify(rockMesh,ratio:0.4,error:0.012),MeshProcessing.simplify(rockMesh,ratio:0.15,error:0.04)])]


        batches.append(SceneBatch(name:"Leaves",mesh:Mesher.leaves(on:crownMesh,count:Int(treeSource.parameters["leaves"] ?? 1800)),instances:crowns.map{var i=$0;i.tint.w=8;return i}))
        for index in batches.indices where ["Trunks","Canopies","Leaves"].contains(batches[index].name) {
            batches[index].roughness=treeSource.appearance["roughness"] ?? -1;batches[index].metallic=treeSource.appearance["metallic"] ?? 0
            for i in batches[index].instances.indices {let kind=batches[index].instances[i].tint.w;batches[index].instances[i].tint *= treeSource.appearance["tint"] ?? 1;batches[index].instances[i].tint.w=kind}
        }

        // A landmark assembled from bounded fields: two leaning uprights and a lintel.
        let arch = Shape.capsule(V3(-3,0,0),V3(-2,8,0),0.85)
            .blended(.capsule(V3(3,0,0),V3(2,8,0),0.85),radius:0.5)
            .blended(.capsule(V3(-2,8,0),V3(2,8,0),1),radius:0.75)
        let ap=V3(terrain.pathX(-67),terrain.height(terrain.pathX(-67),-67),-67)
        batches.append(SceneBatch(name:"Gate",mesh:try mesh(arch,48),instances:[Instance(position:ap,tint:V3(0.58,0.62,0.53),kind:5)]))
        solids.append(Solid(shape:arch,position:ap,scale:1,name:"The old gate"))

        var grass=Mesh(), flowers=Mesh()
        for _ in 0..<24000 {
            let x=rng.range(-76,76), z=rng.range(-108,54)
            if abs(x-terrain.pathX(z)) < rng.range(2.2,3.4) { continue }
            let p=V3(x,terrain.height(x,z)-0.015,z), angle=rng.range(0,6.28)
            let h=rng.range(0.18,0.48), w=rng.range(0.018,0.045)
            let d=V3(cos(angle)*w,0,sin(angle)*w)
            let col=V3(rng.range(0.30,0.49),rng.range(0.49,0.64),rng.range(0.17,0.30))
            let n=normalize(V3(sin(angle)*0.3,0.9,-cos(angle)*0.3))
            let bend=V3(cos(angle+0.8),0,sin(angle+0.8))*h*0.32
            let middle=p+V3(0,h*0.55,0)+bend*0.3,tip=p+V3(0,h,0)+bend
            grass.triangle(Vertex(p-d,n,col),Vertex(p+d,n,col),Vertex(middle+d*0.55,n,col*1.08,wind:0.4))
            grass.triangle(Vertex(p-d,n,col),Vertex(middle+d*0.55,n,col*1.08,wind:0.4),Vertex(middle-d*0.55,n,col*1.08,wind:0.4))
            grass.triangle(Vertex(middle-d*0.55,n,col*1.08,wind:0.4),Vertex(middle+d*0.55,n,col*1.08,wind:0.4),Vertex(tip,n,col*1.18,wind:1))
            if rng.next()<0.065 && z > -60 {
                let fp=p+V3(0,h,0), fc=rng.next()<0.6 ? V3(0.92,0.71,0.68) : V3(0.83,0.85,0.60)
                for j in 0..<5 {
                    let a=Float(j)*1.256, b=a+1.0
                    flowers.triangle(Vertex(fp,V3(0,1,0),fc,wind:1),Vertex(fp+V3(cos(a)*0.16,0.045,sin(a)*0.16),V3(0,1,0),fc,wind:1),Vertex(fp+V3(cos(b)*0.16,0.045,sin(b)*0.16),V3(0,1,0),fc,wind:1))
                }
            }
        }
        batches += [SceneBatch(name:"Meadow",mesh:grass,instances:[Instance(kind:3)]),SceneBatch(name:"Wildflowers",mesh:flowers,instances:[Instance(kind:3)])]
    }

    static var podShape: Shape {
        Shape.sphere(1.15).blended(.sphere(0.72).moved(V3(0,0.75,0)),radius:0.55)
            .blended(.capsule(V3(0,1.1,0),V3(0.48,2.15,0),0.18),radius:0.28)
            .cut(.sphere(0.82).moved(V3(0,0.24,0.85)))
    }
}

func perspective(_ fov: Float, _ aspect: Float, _ near: Float, _ far: Float) -> simd_float4x4 {
    let y=1/tan(fov/2), x=y/aspect, z=far/(near-far)
    return simd_float4x4(columns:(SIMD4(x,0,0,0),SIMD4(0,y,0,0),SIMD4(0,0,z,-1),SIMD4(0,0,z*near,0)))
}
func lookAt(_ eye: V3, _ target: V3, up: V3 = V3(0,1,0)) -> simd_float4x4 {
    let z=normalize(eye-target), x=normalize(cross(up,z)), y=cross(z,x)
    return simd_float4x4(columns:(SIMD4(x.x,y.x,z.x,0),SIMD4(x.y,y.y,z.y,0),SIMD4(x.z,y.z,z.z,0),SIMD4(-dot(x,eye),-dot(y,eye),-dot(z,eye),1)))
}
func orthographic(_ size: Float, _ near: Float, _ far: Float) -> simd_float4x4 {
    simd_float4x4(columns:(SIMD4(1/size,0,0,0),SIMD4(0,1/size,0,0),SIMD4(0,0,1/(near-far),0),SIMD4(0,0,near/(near-far),1)))
}
