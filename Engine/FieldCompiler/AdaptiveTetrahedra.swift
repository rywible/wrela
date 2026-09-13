import FieldCore
import simd

/// Interval-pruned octree sampling with shared face partitions. Each retained
/// cube is a star of tetrahedra from its centre to a conforming boundary fan.
/// Fine faces/edge points are shared explicitly; refinement does not propagate
/// through the body. These are extraction cells, not a simulation mesh.
enum AdaptiveTetrahedra {
  struct Cell {var vertices:SIMD4<Int>;var region:Int;var bounds:Bounds}
  struct Result {var points:[V3];var values:[Float];var cells:[Cell];var subdivisions:Int;var transitionFaces:Int;var zeroClassifiedVertices:Int;var maximumLinearizedZeroDistance:Float}
  struct Leaf {var origin:SIMD3<Int>;var size:Int;var field:Shape}
  struct Face:Hashable {var axis:Int;var plane:Int;var u:Int;var v:Int;var size:Int}
  struct Line:Hashable {var axis:Int;var u:Int;var v:Int}
  static let edges=[(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)]
  static func build(_ shape:Shape,bounds:Bounds,policy:SurfaceSampling,recovery:[SurfaceSamplingRegion]=[]) throws -> Result {
    try policy.validate();try shape.validate()
    try CraftError.require(CraftMath.finite(bounds.min,limit:1000) && CraftMath.finite(bounds.max,limit:1000)
      && (bounds.max-bounds.min).min()>0 && length(bounds.max-bounds.min)<=200,"sampling.bounds","Use finite, positive extraction bounds no larger than 200 m")
    // Integer dyadic coordinates give every shared corner/centre one identity.
    let units=1<<21,extent=(bounds.max-bounds.min).max(),unit=extent/Float(units)
    let origin=(bounds.min+bounds.max)/2-V3(repeating:extent/2)
    func position(_ p:SIMD3<Int>)->V3 {origin+V3(Float(p.x),Float(p.y),Float(p.z))*unit}
    func box(_ p:SIMD3<Int>,_ size:Int)->Bounds {Bounds(position(p),position(p &+ SIMD3(repeating:size)))}
    func possible(_ b:Bounds,_ field:Shape)->Bool {
      let i=field.valueInterval(in:b.expanded(1e-6)),e:Float=1e-6*(1+abs(i.lower)+abs(i.upper))
      return i.lower<=e && i.upper>=(-e)
    }
    var leaves:[Leaf]=[],subdivisions=0
    func visit(_ p:SIMD3<Int>,_ size:Int,_ field:Shape) throws {
      let b=box(p,size)
      if !possible(b,field) {return}
      var target=policy.edgeLength(in:b)
      for r in recovery {
        let closest=max(b.min,min(b.max,r.center))
        if length_squared((closest-r.center)/r.radius)<=1 {target=min(target,r.edgeLength)}
      }
      if Float(size)*unit<=target*1.000001 {
        try CraftError.require(leaves.count<policy.maximumTetrahedra,"sampling.budget","Octree surface cells exceed the volume budget; use larger edge lengths or narrower detail regions")
        leaves.append(Leaf(origin:p,size:size,field:field));return
      }
      try CraftError.require(size>=4,"sampling.convergence","Requested density exceeds the dyadic coordinate depth")
      subdivisions+=1;let half=size/2
      for z in 0...1 {for y in 0...1 {for x in 0...1 {
        let q=p &+ SIMD3(x*half,y*half,z*half),child=box(q,half)
        if possible(child,field) {try visit(q,half,field.specialized(in:child.expanded(2e-6)))}
      }}}
    }
    try visit(.zero,units,shape)
    func faces(_ leaf:Leaf)->[Face] {
      var result:[Face]=[]
      for axis in 0..<3 {let u=(axis+1)%3,v=(axis+2)%3
        for side in 0...1 {result.append(Face(axis:axis,plane:leaf.origin[axis]+side*leaf.size,
          u:leaf.origin[u],v:leaf.origin[v],size:leaf.size))}
      };return result
    }
    let leafFaces=leaves.map(faces)
    var divided:Set<Face>=[]
    for face in leafFaces.joined() {
      var size=face.size*2
      while size<=units {
        divided.insert(Face(axis:face.axis,plane:face.plane,u:face.u/size*size,v:face.v/size*size,size:size));size*=2
      }
    }
    var partitions:[Face:[Face]]=[:],patchSet:Set<Face>=[],transitionFaces=0
    func partition(_ face:Face)->[Face] {
      if let value=partitions[face] {return value}
      let result:[Face]
      if divided.contains(face) {
        let half=face.size/2
        result=(0...1).flatMap{v in (0...1).flatMap{u in partition(Face(axis:face.axis,plane:face.plane,
          u:face.u+u*half,v:face.v+v*half,size:half))}}
      } else {result=[face]}
      partitions[face]=result;return result
    }
    for face in leafFaces.joined() {
      let patches=partition(face);if patches.count>1 {transitionFaces+=1};patchSet.formUnion(patches)
    }
    let patches=patchSet.sorted{a,b in
      for (x,y) in [(a.axis,b.axis),(a.plane,b.plane),(a.u,b.u),(a.v,b.v),(a.size,b.size)] {if x != y {return x<y}};return false
    }
    func coordinate(_ face:Face,_ u:Int,_ v:Int)->SIMD3<Int> {
      var p=SIMD3<Int>.zero;p[face.axis]=face.plane;p[(face.axis+1)%3]=u;p[(face.axis+2)%3]=v;return p
    }
    func corners(_ face:Face)->[SIMD3<Int>] {
      [(0,0),(1,0),(1,1),(0,1)].map{coordinate(face,face.u+$0.0*face.size,face.v+$0.1*face.size)}
    }
    func line(_ a:SIMD3<Int>,_ b:SIMD3<Int>)->Line {
      let axis=(0..<3).first{a[$0] != b[$0]}!
      return Line(axis:axis,u:a[(axis+1)%3],v:a[(axis+2)%3])
    }
    // Matching faces alone is insufficient: a fine cell may meet only an edge.
    // Collect boundary breakpoints on canonical axis lines before triangulating.
    var breaks:[Line:Set<Int>]=[:]
    for face in patches {
      let cs=corners(face)
      for i in 0..<4 {
        let a=cs[i],b=cs[(i+1)%4],key=line(a,b)
        breaks[key,default:[]].formUnion([a[key.axis],b[key.axis]])
      }
    }
    let sortedBreaks=breaks.mapValues{$0.sorted()}
    var points:[V3]=[],values:[Float]=[],pointIDs:[SIMD3<Int>:Int]=[:]
    var zeroClassifiedVertices=0,maximumLinearizedZeroDistance:Float=0
    func id(_ p:SIMD3<Int>)->Int {
      if let i=pointIDs[p] {return i}
      let i=points.count,q=position(p);pointIDs[p]=i;points.append(q)
      var value=shape.value(at:q)
      // A surface arbitrarily near a grid corner otherwise emits microscopic
      // contradictory slivers at Float precision. Classify that corner once,
      // globally, and honour its exact zero in every incident tetrahedron.
      let allowance=4*max(abs(q.x).ulp,max(abs(q.y).ulp,abs(q.z).ulp))
      if value != 0 && abs(value)<allowance*8 {
        let gradient=length(shape.sample(at:q).gradient)
        if gradient>1e-6 && abs(value)/gradient<=allowance {
          zeroClassifiedVertices+=1;maximumLinearizedZeroDistance=max(maximumLinearizedZeroDistance,abs(value)/gradient);value=0
        }
      }
      values.append(value);return i
    }
    var fans:[Face:[SIMD3<Int>]]=[:]
    for face in patches {
      let cs=corners(face),center=id(coordinate(face,face.u+face.size/2,face.v+face.size/2))
      var fan:[SIMD3<Int>]=[]
      for i in 0..<4 {
        let a=cs[i],b=cs[(i+1)%4],key=line(a,b),low=min(a[key.axis],b[key.axis]),high=max(a[key.axis],b[key.axis])
        let coordinates=sortedBreaks[key]!.filter{$0>=low && $0<=high}
        let ordered=a[key.axis]<b[key.axis] ? coordinates:Array(coordinates.reversed())
        for j in 0..<(ordered.count-1) {
          var p=a,q=a;p[key.axis]=ordered[j];q[key.axis]=ordered[j+1]
          fan.append(SIMD3(center,id(p),id(q)))
        }
      };fans[face]=fan
    }
    var cells:[Cell]=[]
    for (region,leaf) in leaves.enumerated() {
      let center=id(leaf.origin &+ SIMD3(repeating:leaf.size/2))
      for face in leafFaces[region] {for patch in partitions[face]! {for triangle in fans[patch]! {
        let ids=SIMD4(center,triangle.x,triangle.y,triangle.z)
        let p=points[ids.x],q=points[ids.y],r=points[ids.z],s=points[ids.w]
        let bounds=Bounds(min(min(p,q),min(r,s)),max(max(p,q),max(r,s)))
        // The final polygon extractor consumes only sign-crossing tetrahedra.
        // Discard the others now, after the requested source sampling depth has
        // been reached. This is exactly its existing no-polygon condition.
        let negative=(0..<4).contains{values[ids[$0]]<0}
        let positive=(0..<4).contains{values[ids[$0]]>=0}
        if !negative || !positive {continue}
        try CraftError.require(cells.count<policy.maximumTetrahedra,"sampling.budget",
          "Conforming octree extraction exceeds \(policy.maximumTetrahedra) tetrahedra (\(leaves.count) surface cells, \(transitionFaces) transition faces). Increase edge lengths, narrow detail regions, or explicitly increase the volume budget")
        cells.append(Cell(vertices:ids,region:region,bounds:bounds))
      }}}
    }
    return Result(points:points,values:values,cells:cells,subdivisions:subdivisions,transitionFaces:transitionFaces,
      zeroClassifiedVertices:zeroClassifiedVertices,maximumLinearizedZeroDistance:maximumLinearizedZeroDistance)
  }
}
