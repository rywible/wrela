import simd

/// A local differential explanation of the authoritative composed field.
/// Response is outward surface motion per unit increase of a source parameter,
/// from implicit differentiation. It is not a finite-edit or topology guarantee.
public struct AnatomyParameterResponse:Codable,Sendable {
  public var parameter:String
  public var unit:String
  public var normalResponse:Float
}
public struct AnatomyInfluence:Codable,Sendable {
  public var element:String
  public var region:String
  public var operation:AnatomyOperation
  public var fieldCoefficient:Float
  public var primitiveValue:Float
  public var controls:[AnatomyParameterResponse]
}
public struct AnatomyExplanation:Codable,Sendable {
  public var fieldValue:Float
  public var gradientMagnitude:Float
  public var normal:V3
  public var linearizedSurfaceOffset:Float?
  public var contributors:[AnatomyInfluence]
  public var inactiveElements:[String]
  public var limits:String
}
extension AnatomySource {
  public func explain(at point:V3) throws -> AnatomyExplanation {
    try validate()
    try CraftError.require(CraftMath.finite(point,limit:200),"anatomy.explain.point","Use a finite bind point")
    let elements=surfaceElements
    let samples=elements.map{$0.shape.sample(at:point)}
    // Forward composition followed by reverse chain rule. In a cut the
    // primitive coefficient is negative; buried inputs have zero influence.
    var value=samples[0].value,oldFactors=[Float(0)],newFactors=[Float(1)],blendDerivatives=[Float(0)]
    for i in 1..<elements.count {
      let element=elements[i],next=samples[i].value
      if element.operation == .cut {
        if let k=element.cutBlend,k>0 {
          let h=clamp(0.5+0.5*(value+next)/k,0,1)
          oldFactors.append(h);newFactors.append(-(1-h));blendDerivatives.append(h*(1-h))
          value = -next+(value+next)*h+k*h*(1-h)
        } else {
          let old=value >= -next
          oldFactors.append(old ? 1:0);newFactors.append(old ? 0:-1);blendDerivatives.append(0)
          value=max(value,-next)
        }
      } else if element.blend>0 {
        let h=clamp(0.5+0.5*(next-value)/element.blend,0,1)
        oldFactors.append(h);newFactors.append(1-h);blendDerivatives.append(-h*(1-h))
        value=mix(next,value,h)-element.blend*h*(1-h)
      } else {
        let old=value<=next
        oldFactors.append(old ? 1:0);newFactors.append(old ? 0:1);blendDerivatives.append(0)
        value=min(value,next)
      }
    }
    let composed=sample(at:point),magnitude=length(composed.field.gradient)
    var coefficients=Array(repeating:Float(0),count:elements.count),blend=coefficients,downstream:Float=1
    for i in elements.indices.reversed() {
      coefficients[i]=downstream*newFactors[i];blend[i]=downstream*blendDerivatives[i]
      downstream *= oldFactors[i]
    }
    var contributors:[AnatomyInfluence]=[],inactive:[String]=[]
    for i in elements.indices {
      let element=elements[i],coefficient=coefficients[i]
      guard abs(coefficient)>0.000001 || abs(blend[i])>0.000001 else {inactive.append(element.id);continue}
      var controls:[AnatomyParameterResponse]=[]
      func response(_ parameter:String,_ derivative:Float,unit:String="metre") {
        if magnitude>0.00001 {controls.append(AnatomyParameterResponse(parameter:parameter,unit:unit,normalResponse:-derivative/magnitude))}
      }
      // Primitive-only differences keep this O(number of elements), independent
      // of the length of the field expression. Composition factors are analytic.
      func difference(_ parameter:String,_ axis:Int,step:Float,_ change:(inout AnatomyElement,Int,Float)->Void,unit:String="metre") {
        var lo=element,hi=element;change(&lo,axis,-step);change(&hi,axis,step)
        response(parameter,coefficient*(hi.shape.value(at:point)-lo.shape.value(at:point))/(2*step),unit:unit)
      }
      for axis in 0..<3 {
        let suffix=["x","y","z"][axis]
        difference("center."+suffix,axis,step:0.0001,{$0.center[$1]+=$2})
        if element.primitive == .ellipsoid {
          difference("radius."+suffix,axis,step:min(0.0001,element.radius[axis]*0.01),{$0.radius[$1]+=$2})
          difference("rotation."+suffix,axis,step:0.02,{$0.rotation[$1]+=$2},unit:"degree")
        } else {difference("end."+suffix,axis,step:0.0001,{$0.end[$1]+=$2})}
      }
      if element.primitive == .capsule {
        response("radius",-coefficient)
      }
      if i>0 && element.operation == .add && element.blend>0 {response("blend",blend[i])}
      if i>0 && element.operation == .cut && (element.cutBlend ?? 0)>0 {response("cutBlend",blend[i])}
      contributors.append(AnatomyInfluence(element:element.id,region:element.region,operation:element.operation,
        fieldCoefficient:coefficient,primitiveValue:samples[i].value,controls:controls))
    }
    return AnatomyExplanation(fieldValue:value,gradientMagnitude:magnitude,normal:composed.field.normal,
      linearizedSurfaceOffset:magnitude>0.00001 ? -value/magnitude:nil,contributors:contributors,inactiveElements:inactive,
      limits:"Local first-order response of base anatomy in bind space. Positive response moves outward. Hard branch ties, large edits, sculpt layers, skin deformation and topology changes require new rendered inspection. blend affects additions; cutBlend controls subtraction rounding.")
  }
}
