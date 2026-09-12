import FieldCore
import simd

public enum ClothCompiler {
  public static func compile(_ panel:ClothPanel,rig:SecondaryRig) throws -> (Mesh,[SkinWeight],[String]) {
    try panel.validate(joints:Set(rig.nodes.map(\.joint)))
    let nodes=Dictionary(uniqueKeysWithValues:rig.nodes.map{($0.id,$0.position)}),ids=panel.points.indices.map{panel.id+"-\($0)"}
    try CraftError.require(ids.allSatisfy{nodes[$0] != nil},"clothPanel.\(panel.id)","Missing authored guide nodes")
    let points=ids.map{nodes[$0]!}
    func position(_ u:Float,_ v:Float)->V3 {
      let row=(0..<panel.rows).map{j in GuideGroom.point(Array(points[(j*panel.columns)..<((j+1)*panel.columns)]),u)}
      return GuideGroom.point(row,v)
    }
    let mesh=ParametricMesh.surface(u:panel.resolution,v:panel.resolution,color:panel.color,position:position)
    let weights=mesh.vertices.indices.map{i in SurfaceSkinning.weights(uv:SIMD2(Float(i%(panel.resolution+1))/Float(panel.resolution),Float(i/(panel.resolution+1))/Float(panel.resolution)),columns:panel.columns,rows:panel.rows)}
    return(mesh,weights,ids)
  }
}
