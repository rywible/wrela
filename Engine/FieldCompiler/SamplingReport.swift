import Foundation
public struct SamplingReport:Codable,Equatable,Sendable {
  public var volumeVertices:Int
  public var tetrahedra:Int
  public var cellSubdivisions:Int
  public var transitionFaces:Int
  public var recoveryPasses:Int
  public var zeroClassifiedVertices:Int
  public var maximumLinearizedZeroDistance:Float
  public var surfaceVertices:Int
  public var triangles:Int
  public var refinedRoots:Int
  public var heldRoots:Int
  public var orientedEdgeIncidence:Bool
  public var degenerateTriangles:Int
  public var removedDegenerateTriangles:Int
  public var sampledInvertedTriangles:Int
  public var milliseconds:Double
}
public struct SampledSurface:Sendable {
  public var mesh:Mesh
  public var report:SamplingReport
}
