import FieldCore
import simd

public enum SkinFieldCompiler {
  public static func mix(_ inputs:[(SkinWeight,Float)])->SkinWeight {
    var values:[UInt32:Float]=[:]
    for (skin,amount) in inputs {for k in 0..<4 {values[skin.joints[k],default:0]+=skin.weights[k]*amount}}
    let sorted=values.filter{$0.value>0}.sorted{a,b in a.value==b.value ? a.key<b.key:a.value>b.value}.prefix(4)
    let total=sorted.reduce(Float(0)){$0+$1.value}
    var output=SkinWeight(0);output.weights = .zero
    for (k,pair) in sorted.enumerated() {output.joints[k]=pair.key;output.weights[k]=pair.value/max(total,1e-12)}
    return output
  }
  public static func apply(_ fields:[SkinField],mesh:Mesh,joints:[String],weights:[SkinWeight],fallback:String) throws -> ([String],[SkinWeight]) {
    guard !fields.isEmpty else {return(joints,weights)}
    var ids=joints.isEmpty ? [fallback]:joints
    for field in fields {for id in field.jointWeights.keys.sorted() where !ids.contains(id) {ids.append(id)}}
    try CraftError.require(ids.count<=64,"skin.palette","A draw supports at most 64 joints")
    var output=weights.isEmpty ? Array(repeating:SkinWeight(0),count:mesh.vertices.count):weights
    for field in fields {
      var target=SkinWeight(0);target.weights = .zero
      for (k,id) in field.jointWeights.keys.sorted().enumerated() {target.joints[k]=UInt32(ids.firstIndex(of:id)!);target.weights[k]=field.jointWeights[id]!}
      var count=0
      for i in mesh.vertices.indices {
        let p=mesh.vertices[i].position,w=field.coverage(V3(p.x,p.y,p.z))
        if w>1e-7 {output[i]=mix([(output[i],1-w),(target,w)]);count+=1}
      }
      try CraftError.require(count>0,"skinField.\(field.id)","No vertices affected")
    }
    return(ids,output)
  }
}
