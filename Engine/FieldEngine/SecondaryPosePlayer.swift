import FieldCore
import Foundation
import simd

/// Instance-owned replay cache: seeking is simulation from a known checkpoint,
/// never an approximation based on the editor's previous scrub direction.
package final class SecondaryPosePlayer {
  package init() {}
  private var key = ""
  private var tick = 0
  private var checkpoints:[Int:SecondaryState] = [:]
  private var state = SecondaryState([])
  private var program:SecondaryProgram?
  private var adjacency:[[Int]] = []
  private var curves:[[Int]] = []
  package private(set) var metrics = SecondaryMetrics()
  package private(set) var simulatedTicks = 0
  package private(set) var positions:[V3] = []
  package func evaluate(key:String,rig:SecondaryRig,settings:SecondarySettings,time:Float,
    sample:(Float)->[String:simd_float4x4],colliders:[RigCapsule],height:(Float,Float)->Float)->[String:simd_float4x4] {
    func anchors(_ pose:[String:simd_float4x4])->[V3] {
      rig.nodes.map {n in let p=(pose[n.joint] ?? matrix_identity_float4x4)*SIMD4(n.position,1);return V3(p.x,p.y,p.z)}
    }
    let end=max(0,Int((time*60).rounded()))
    var pose=sample(Float(end)/60)
    simulatedTicks=0
    guard !rig.nodes.isEmpty else {
      positions=[];metrics=SecondaryMetrics();program=nil
      return pose
    }
    // The endpoint is often also the next simulation tick. Evaluate its body
    // constraints once and reuse the exact result instead of solving twice.
    func sampleAt(_ t:Int)->[String:simd_float4x4] {t==end ? pose:sample(Float(t)/60)}
    var initialPose:[String:simd_float4x4]?
    if self.key != key || program?.rig != rig || program?.settings != settings {
      self.key=key;tick=0
      program=SecondaryProgram(rig:rig,settings:settings)
      adjacency=rig.nodes.map{_ in []}
      for e in rig.links {adjacency[e.a].append(e.b);adjacency[e.b].append(e.a)}
      // Structural contact edges identify curve neighbours; bend constraints
      // must not become orientation neighbours. Branches retain the old frame.
      var links=rig.nodes.map{_ in [Int]()}
      for e in rig.links where e.contact && rig.nodes[e.a].frame == .curve && rig.nodes[e.b].frame == .curve {
        links[e.a].append(e.b);links[e.b].append(e.a)
      }
      curves=[];var visited=Set<Int>()
      for start in links.indices where links[start].count==1 && !visited.contains(start) {
        var path:[Int]=[],previous = -1,current=start
        while !visited.contains(current) && links[current].count<=2 {
          path.append(current);visited.insert(current)
          guard let next=links[current].first(where:{$0 != previous}) else {break}
          previous=current;current=next
        }
        if path.count>=2 && links[path.last!].count==1 {curves.append(path)}
      }
      let initial=sampleAt(0),targets=anchors(initial)
      initialPose=initial
      state=SecondaryState(targets)
      if settings.enabled {
        let bodies=colliders.map{$0.transformed(initial[$0.joint] ?? matrix_identity_float4x4)}
        for _ in 0..<120 {SecondaryMotion.step(&state,program:program!,from:targets,to:targets,bodies:bodies,height:height,time:0)}
      }
      checkpoints=[0:state]
    }
    if end<tick {
      tick=checkpoints.keys.filter{$0<=end}.max() ?? 0;state=checkpoints[tick]!
    }
    if settings.enabled {
      let startPose=initialPose ?? sampleAt(tick)
      var previous=anchors(startPose)
      var previousBodies=colliders.map{$0.transformed(startPose[$0.joint] ?? matrix_identity_float4x4)}
      while tick<end {
        let t=Float(tick+1)/60,nextPose=sampleAt(tick+1),next=anchors(nextPose)
        let bodies=colliders.map{$0.transformed(nextPose[$0.joint] ?? matrix_identity_float4x4)}
        SecondaryMotion.step(&state,program:program!,from:previous,to:next,bodies:bodies,height:height,time:t,previousBodies:previousBodies)
        previousBodies=bodies
        previous=next;tick+=1;simulatedTicks+=1
        if tick%60==0 {checkpoints[tick]=state
          if checkpoints.count>121,let oldest=checkpoints.keys.filter({$0>0}).min() {checkpoints.removeValue(forKey:oldest)}
        }
      }
    } else {state=SecondaryState(anchors(pose));tick=end}
    let targets=anchors(pose),bodies=colliders.map{$0.transformed(pose[$0.joint] ?? matrix_identity_float4x4)}
    metrics=SecondaryMotion.metrics(state,rig:rig,targets:targets,bodies:bodies,height:height)
    positions=state.positions
    var curveFrames:[Int:simd_float3x3]=[:]
    for curve in curves {
      if let frames=GuideFrame.curve(old:curve.map{targets[$0]},new:curve.map{state.positions[$0]}) {
        for (i,node) in curve.enumerated() {curveFrames[node]=frames[i]}
      }
    }
    for (i,n) in rig.nodes.enumerated() {
      let neighbors=adjacency[i]
      var rotation=simd_quatf(angle:0,axis:V3(0,1,0))
      if let first=neighbors.first {
        let old=targets[first]-targets[i],new=state.positions[first]-state.positions[i]
        if length_squared(old)>1e-8 && length_squared(new)>1e-8 {
          let axis=normalize(new)
          rotation=simd_quatf(from:normalize(old),to:axis)
          if let second=neighbors.dropFirst().first(where:{length(cross(normalize(targets[$0]-targets[i]),normalize(old)))>0.3}) {
            let a=rotation.act(targets[second]-targets[i]),b=state.positions[second]-state.positions[i]
            let ap=a-axis*dot(a,axis),bp=b-axis*dot(b,axis)
            if length_squared(ap)>1e-8 && length_squared(bp)>1e-8 {
              let angle=atan2(dot(axis,cross(normalize(ap),normalize(bp))),dot(normalize(ap),normalize(bp)))
              rotation=simd_quatf(angle:angle,axis:axis)*rotation
            }
          }
        }
      }
      func translation(_ v:V3)->simd_float4x4 {var m=matrix_identity_float4x4;m.columns.3=SIMD4(v,1);return m}
      var differential=simd_float4x4(rotation)
      if let frame=curveFrames[i] {
        differential=simd_float4x4(columns:(SIMD4(frame[0],0),SIMD4(frame[1],0),SIMD4(frame[2],0),SIMD4(0,0,0,1)))
      }
      if n.frame == .surface,let affine=GuideFrame.surface(old:neighbors.map{targets[$0]-targets[i]},new:neighbors.map{state.positions[$0]-state.positions[i]}) {
        differential=simd_float4x4(columns:(SIMD4(affine[0],0),SIMD4(affine[1],0),SIMD4(affine[2],0),SIMD4(0,0,0,1)))
      }
      let correction=translation(state.positions[i])*differential*translation(-targets[i])
      pose[n.id]=correction*(pose[n.joint] ?? matrix_identity_float4x4)
    }
    return pose
  }
}
