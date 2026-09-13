import simd

/// Immutable acceleration data derived entirely from an anatomy source. This
/// caches primitive transforms and section interpolation; it changes no field,
/// region, material or skin blending policy and owns no editor state.
public struct AnatomySampler:Sendable {
  private let surface:[AnatomyElement]
  private let shapes:[Shape]
  private let part:String
  private let skinBlend:Float
  public init(_ source:AnatomySource) {
    surface=source.surfaceElements;shapes=surface.map(\.shape)
    part=source.part;skinBlend=source.skinBlend
  }
  /// Material and weight interpolation follows the same smooth CSG partition as
  /// geometry. A cut owns its visible region, but inherits skin weights when it
  /// has no explicit weight assignment, avoiding unweighted socket interiors.
  public func sample(at point:V3)->AnatomySample {
    guard let first=surface.first else {return AnatomySample(field:FieldSample(.infinity,.zero),element:"",region:"",color:.zero,jointWeights:[:])}
    func weights(_ element:AnatomyElement)->[String:Float] {element.jointWeights.isEmpty ? [part:1]:element.jointWeights}
    var result=AnatomySample(field:shapes[0].sample(at:point),element:first.id,region:first.region,color:first.color,jointWeights:weights(first))
    for i in 1..<surface.count {
      let element=surface[i],next=shapes[i].sample(at:point)
      if element.operation == .cut {
        if let k=element.cutBlend,k>0 {
          let old=result.field,h=clamp(0.5+0.5*(old.value+next.value)/k,0,1)
          result.field=FieldSample(-next.value+(old.value+next.value)*h+k*h*(1-h),-next.gradient+(old.gradient+next.gradient)*h)
          result.color=element.color+(result.color-element.color)*h
          if !element.jointWeights.isEmpty {
            var merged=result.jointWeights.mapValues{$0*h}
            for (id,weight) in element.jointWeights {merged[id,default:0]+=weight*(1-h)}
            result.jointWeights=merged.filter{$0.value>0.0000001}
          }
          if h<0.5 {result.element=element.id;result.region=element.region}
        } else if -next.value>result.field.value {
          result.field=FieldSample(-next.value,-next.gradient);result.element=element.id;result.region=element.region;result.color=element.color
          if !element.jointWeights.isEmpty {result.jointWeights=element.jointWeights}
        }
      } else {
        let h=element.blend>0 ? clamp(0.5+0.5*(next.value-result.field.value)/element.blend,0,1):(result.field.value<=next.value ? Float(1):Float(0))
        let skinRadius=max(element.blend,skinBlend)
        let skinH=skinRadius>0 ? clamp(0.5+0.5*(next.value-result.field.value)/skinRadius,0,1):h
        let old=result.field
        result.field=FieldSample(mix(next.value,old.value,h)-element.blend*h*(1-h),next.gradient+(old.gradient-next.gradient)*h)
        result.color=element.color+(result.color-element.color)*h
        var merged=result.jointWeights.mapValues{$0*skinH}
        for (id,weight) in weights(element) {merged[id,default:0]+=weight*(1-skinH)}
        result.jointWeights=merged.filter{$0.value>0.0000001}
        if h<0.5 {result.element=element.id;result.region=element.region}
      }
    }
    return result
  }
}
