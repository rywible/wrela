import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import simd

extension WorkshopRenderer {
  var rehearsalActive: Bool {creatureWorkshop.rehearsal != RehearsalSurface()}
  func rehearsalItems(center: V3) -> [RenderItem] {
    var items:[RenderItem]=[]
    let origin=V3(center.x,studioBounds.min.y,center.z),scale=studioLayout.scale
    let surface=creatureWorkshop.rehearsal
    if rehearsalActive {
      if rehearsalMesh == nil || rehearsalMeshSurface != surface {
        var surfaces:[Mesh]=[]
        for raised in [true,false] {
          let a:Float=raised ? -6:surface.stepZ,b:Float=raised ? surface.stepZ:6
          surfaces.append(ParametricMesh.surface(u:24,v:24,color:V3(repeating:0.43)) {u,v in
            let x = -6+12*u,z=b+(a-b)*v
            return V3(x,surface.slopeX*x+surface.slopeZ*z+(raised ? surface.stepHeight:0),z)
          })
        }
        if surface.stepHeight>0 {
          surfaces.append(ParametricMesh.surface(u:24,v:1,color:V3(0.32,0.35,0.36)) {u,v in
            let x = -6+12*u,z=surface.stepZ
            return V3(x,surface.slopeX*x+surface.slopeZ*z+v*surface.stepHeight,z)
          })
        }
        var b=SceneBatch(name:"Rehearsal floor",mesh:ParametricMesh.joined(surfaces),instances:[Instance(kind:7)])
        b.doubleSided=true;rehearsalMesh=upload(b);rehearsalMeshSurface=surface
      }
      if let batch=rehearsalMesh {
        items.append(RenderItem(batch:batch,instance:Instance(position:-origin*scale,scale:V3(repeating:scale),kind:7)))
      }
    }
    if creatureWorkshop.contactView {
      let pose=creatureWorkshop.evaluatedPose(source:studioSource)
      let contacts=pose.contacts
      func wire(_ a:V3,_ b:V3,_ color:V3) {
        guard length(b-a)>0.0001 else {return}
        var line=Instance(tint:color,kind:7)
        line.model=transform(((a+b)/2-origin)*scale,V3(repeating:1),0)
          * simd_float4x4(simd_quatf(from:V3(0,1,0),to:normalize(b-a)))
          * simd_float4x4(diagonal:SIMD4(0.003*scale,length(b-a)*scale/0.02,0.003*scale,1))
        items.append(RenderItem(batch:floor,instance:line,castsShadow:false))
      }
      if creatureWorkshop.rigView {
        _ = secondaryPose()
        let rig=studioSource.resolvedSecondaryRig,points=secondaryPlayer.positions
        if points.count==rig.nodes.count {
          for e in rig.links {
            let rest=length(rig.nodes[e.a].position-rig.nodes[e.b].position)
            let strain=abs(length(points[e.a]-points[e.b])-rest)/rest
            let id=rig.nodes[e.a].id+" → "+rig.nodes[e.b].id
            let contact=secondaryPlayer.metrics.maximumEdgePenetration>0.0005 && secondaryPlayer.metrics.worstEdgePenetration==id
            wire(points[e.a],points[e.b],contact ? V3(0.95,0.1,0.6):strain>0.05 ? V3(0.95,0.2,0.05):V3(0.25,0.6,0.9))
          }
        }
        for source in studioSource.resolvedColliders {
          let body=source.transformed(pose.matrices[source.joint] ?? matrix_identity_float4x4)
          let tangent=length(body.b-body.a)>0.001 ? normalize(body.b-body.a):V3(0,1,0)
          let u=normalize(cross(tangent,abs(tangent.y)<0.9 ? V3(0,1,0):V3(1,0,0)))
          let v=cross(tangent,u),color=V3(0.7,0.32,0.16)
          for endpoint in [body.a,body.b] {
            for i in 0..<16 {
              let a=Float(i)*2*Float.pi/16,b=Float(i+1)*2*Float.pi/16
              wire(endpoint+body.radius*(cos(a)*u+sin(a)*v),endpoint+body.radius*(cos(b)*u+sin(b)*v),color)
            }
          }
          for d in [u,v,-u,-v] {wire(body.a+d*body.radius,body.b+d*body.radius,color)}
        }
      }
      func marker(_ p:V3,_ color:V3,_ size:Float) {
        items.append(RenderItem(batch:floor,instance:Instance(position:(p-origin)*scale,
          scale:V3(size/4,size/0.02,size/4)*scale,tint:color,kind:7),castsShadow:false))
      }
      for c in contacts {
        marker(c.actual,c.residual>0.002 ? V3(0.95,0.12,0.05):c.planted ? V3(0.12,0.8,0.35):V3(0.2,0.5,0.95),0.08)
        marker(c.target,V3(0.95,0.66,0.08),0.035)
        let a=c.target-V3(0,0.18,0),b=a+c.normal*0.4
        var line=Instance(tint:V3(0.2,0.75,0.85),kind:7)
        line.model=transform(((a+b)/2-origin)*scale,V3(repeating:1),0)
          * simd_float4x4(simd_quatf(from:V3(0,1,0),to:c.normal))
          * simd_float4x4(diagonal:SIMD4(0.005*scale,0.4*scale/0.02,0.005*scale,1))
        items.append(RenderItem(batch:floor,instance:line,castsShadow:false))
      }
    }
    return items
  }
}
