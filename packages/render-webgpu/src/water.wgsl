// Specialize away water's response programs from opaque pipelines, and keep
// dense references and coherent orbit work out of the ordinary water kernel.
override WATER_SURFACE:bool=false;
override WATER_COHERENT:bool=false;
override WATER_REFERENCE:bool=false;
// Per-invocation memoization is exact: only equal world positions reuse source
// transmission. Temporal quadrature and coherent phase orbits hit this cache.
var<private> waterSunCachePosition:vec3f;
var<private> waterSunCacheValue:vec3f;
var<private> waterSunCacheValid:bool;
fn waterSunTransmission(world:vec3f)->vec3f {
  if(!waterSunCacheValid||any(world!=waterSunCachePosition)) {
    waterSunCacheValue=physicalCachedSunTransmittance(world);waterSunCachePosition=world;waterSunCacheValid=true;
  }
  return waterSunCacheValue;
}
// This program integrates the authored water response in one correlated pixel/shutter box.
// Production selection uses the actual footprint, exact carrier relations and per-query checks.
// The geometry low-pass above is deliberately not used for micro appearance.
fn authoredWaterSlope(xz:vec2f,time:f32)->vec2f {
  if(obj.glintTime.z>0.5) {return glintSlope(glintPhase(vec3f(xz.x,0.0,xz.y),time));}
  var slope=vec2f(0.0);
  for(var i=0u;i<min(u32(obj.flags.w),8u);i++) {
    let wave=obj.waves[i];
    let k=6.28318530718/wave.shape.y;
    let direction=vec2f(cos(wave.shape.w),sin(wave.shape.w));
    let phase=k*(dot(direction,xz)-wave.shape.z*time)+wave.phase.x;
    slope+=wave.shape.x*k*direction*cos(phase);
  }
  return slope;
}
fn waterSinc(x:f32)->f32 {
  if(abs(x)<0.001) {return 1.0-x*x/6.0+x*x*x*x/120.0;}
  return sin(x)/x;
}
// Exact finite-box first slope moment; this is NOT an integrated nonlinear BRDF.
fn waterMeanSlope(xz:vec2f,dx:vec2f,dy:vec2f,shutter:f32)->vec2f {
  var slope=vec2f(0.0);
  for(var i=0u;i<min(u32(obj.flags.w),8u);i++) {
    let wave=obj.waves[i];let k=6.28318530718/wave.shape.y;
    let direction=vec2f(cos(wave.shape.w),sin(wave.shape.w));
    let phase=k*(dot(direction,xz)-wave.shape.z*g.params.x)+wave.phase.x;
    let factor=waterSinc(k*dot(direction,dx)*0.5)*waterSinc(k*dot(direction,dy)*0.5)*waterSinc(-k*wave.shape.z*shutter*0.5);
    slope+=wave.shape.x*k*direction*cos(phase)*factor;
  }
  return slope;
}
fn waterLighting(value:vec3f)->mat2x3f {return mat2x3f(vec3f(0.0),value);}
fn waterRadianceLength(value:mat2x3f)->f32 {return max(length(value[0]),length(value[1]));}
fn waterRadiancePositive(value:mat2x3f)->mat2x3f {return mat2x3f(max(value[0],vec3f(0.0)),max(value[1],vec3f(0.0)));}
// The complete response is shared by finite-box quadrature and the phase polynomial.
fn waterNonSunResponseAtNormal(world:vec3f,n:vec3f,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  let view=normalize(g.camera.xyz-world);
  let nv=clamp(dot(n,view),0.0,1.0);
  let fresnel=0.02037+0.97963*pow(1.0-nv,5.0);let f0=vec3f(0.02037);
  let sky=physicalSkyRoughReflection(reflect(-view,n),rough)*environmentSpecularWeight(nv,rough,f0);
  var color=vec3f(0.0);
  for(var i=0u;i<min(u32(g.viewport.z),8u);i++) {
    if((u32(obj.localLighting.x)&(1u<<i))==0u){continue;}
    let point=g.points[i];let offset=point.position.xyz-world;
    let distanceSquared=max(dot(offset,offset),0.01);let light=offset*inverseSqrt(distanceSquared);
    let attenuation=localLightAttenuation(distanceSquared,point.color.w);if(attenuation==0.0){continue;}
    let pointVisibility=localLightVisibility(i,world,n);
    let pointNL=max(dot(n,light),0.0);let diffuse=base*(1.0-metallic)/BRDF_PI;
    color+=(diffuse*pointNL+brdfGGX(n,view,light,rough,f0))*point.color.xyz*point.position.w*attenuation*pointVisibility;
  }
  return mat2x3f(sky,color);
}
fn waterNonSunResponseAtSlope(world:vec3f,slope:vec2f,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  return waterNonSunResponseAtNormal(world,normalize(vec3f(-slope.x,1.0,-slope.y)),base,rough,metallic);
}
// The conventional material path retains the scene's authored layered/bump
// normal when those correlations are outside the phase-program domain.
fn waterDirectMaterialResponse(world:vec3f,n:vec3f,view:vec3f,base:vec3f,rough:f32,metallic:f32)->vec3f {
  let response=waterNonSunResponseAtNormal(world,n,base,rough,metallic);
  let sun=brdfGGX(n,view,g.sun.xyz,rough,vec3f(0.02037))*g.sunlight.xyz*g.sun.w*waterSunTransmission(world)*shadow(world,n);
  return waterTransportWithSky(world,n,view,base,rough,response[0])+response[1]+sun;
}
fn waterResponseAtSlope(world:vec3f,slope:vec2f,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  let n=normalize(vec3f(-slope.x,1.0,-slope.y));let view=normalize(g.camera.xyz-world);
  return waterNonSunResponseAtSlope(world,slope,base,rough,metallic)+waterLighting(brdfGGX(n,view,g.sun.xyz,rough,vec3f(0.02037))*g.sunlight.xyz*g.sun.w*waterSunTransmission(world)*shadow(world,n));
}
fn authoredWaterResponse(world:vec3f,time:f32,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  return waterResponseAtSlope(world,authoredWaterSlope(world.xz,time),base,rough,metallic);
}
struct WaterPhaseBox {
  origin:array<f32,8>,dx:array<f32,8>,dy:array<f32,8>,dt:array<f32,8>,slopes:array<vec2f,8>,
  count:u32,shutter:f32,
};
fn prepareWaterPhaseBox(world:vec3f,dx:vec3f,dy:vec3f)->WaterPhaseBox {
  var box:WaterPhaseBox;box.count=min(u32(obj.flags.w),8u);box.shutter=max(obj.waves[1].phase.y,0.0);
  for(var i=0u;i<box.count;i++) {
    let wave=obj.waves[i];let k=BRDF_TAU/wave.shape.y;let carrier=k*vec2f(cos(wave.shape.w),sin(wave.shape.w));
    box.origin[i]=dot(carrier,world.xz)-k*wave.shape.z*g.params.x+wave.phase.x;
    box.dx[i]=dot(carrier,dx.xz);box.dy[i]=dot(carrier,dy.xz);box.dt[i]=-k*wave.shape.z*box.shutter;
    box.slopes[i]=wave.shape.x*carrier;
  }
  return box;
}
fn waterBoxSlope(box:WaterPhaseBox,offset:vec3f)->vec2f {
  var slope=vec2f(0.0);
  for(var i=0u;i<box.count;i++) {
    let phase=box.origin[i]+box.dx[i]*offset.x+box.dy[i]*offset.y+box.dt[i]*offset.z;
    slope+=box.slopes[i]*cos(phase);
  }
  return slope;
}
fn waterAxisRate(box:WaterPhaseBox,index:u32,axis:u32)->f32 {
  if(axis==0u) {return box.dx[index];}if(axis==1u) {return box.dy[index];}return box.dt[index];
}
// Exact f32 equality only. No near-carrier merge and no independent phase randomization.
fn waterCommonRate(box:WaterPhaseBox,axis:u32)->f32 {
  var rate=0.0;
  for(var i=0u;i<box.count;i++) {
    if(dot(box.slopes[i],box.slopes[i])==0.0) {continue;}
    let next=waterAxisRate(box,i,axis);if(next==0.0) {continue;}
    if(rate!=0.0&&rate!=next) {return 0.0;}rate=next;
  }
  return rate;
}
fn waterConditionalOrbit(box:WaterPhaseBox,axis:u32,offset:vec3f)->PhaseEllipse {
  var orbit:PhaseEllipse;
  for(var i=0u;i<box.count;i++) {
    let phase=box.origin[i]+box.dx[i]*offset.x+box.dy[i]*offset.y+box.dt[i]*offset.z;
    if(waterAxisRate(box,i,axis)==0.0) {orbit.mean+=box.slopes[i]*cos(phase);}
    else {orbit.a+=box.slopes[i]*cos(phase);orbit.b-=box.slopes[i]*sin(phase);}
  }
  return orbit;
}
fn waterEllipseResponse(world:vec3f,orbit:PhaseEllipse,angle:f32,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  return waterResponseAtSlope(world,phaseEllipseSlope(orbit,vec2f(cos(angle),sin(angle))),base,rough,metallic);
}
// Coefficients are fitted locally to this query's complete response, so changing
// the camera/light/material cannot reuse stale coefficients. Allocation is fixed.
struct WaterPhasePolynomial { constant:mat2x3f,re:array<mat2x3f,7>,im:array<mat2x3f,7> };
fn fitWaterPhasePolynomial(world:vec3f,orbit:PhaseEllipse,base:vec3f,rough:f32,metallic:f32)->WaterPhasePolynomial {
  var program:WaterPhasePolynomial;
  for(var i=0u;i<16u;i++) {
    let angle=BRDF_TAU*(f32(i)+0.5)/16.0;
    let response=waterEllipseResponse(world,orbit,angle,base,rough,metallic)*(1.0/16.0);
    program.constant+=response;
    for(var k=0u;k<7u;k++) {
      let phase=f32(k+1u)*angle;
      program.re[k]+=response*cos(phase);program.im[k]-=response*sin(phase);
    }
  }
  return program;
}
fn evaluateWaterPhasePolynomial(program:WaterPhasePolynomial,center:f32,width:f32)->mat2x3f {
  var result=program.constant;
  for(var k=0u;k<7u;k++) {
    let harmonic=f32(k+1u);let phase=harmonic*center;
    result+=2.0*(program.re[k]*cos(phase)-program.im[k]*sin(phase))*waterSinc(harmonic*width*0.5);
  }
  return result;
}
fn waterWarpedSun(world:vec3f,orbit:PhaseEllipse,p:PhaseGGX,q:PhaseWarp,nodes:u32,shift:f32)->vec3f {
  let ratio=q.low/q.high;var result=vec3f(0.0);
  for(var i=0u;i<nodes;i++) {
    let angle=BRDF_TAU*(f32(i)+shift)/f32(nodes);let cs=vec2f(cos(angle),sin(angle));
    let denominator=1.0+cs.x+ratio*(1.0-cs.x);
    let mapped=vec2f(1.0+cs.x-ratio*(1.0-cs.x),2.0*sqrt(ratio)*cs.y)/denominator;
    let axis=vec2f(mapped.x*q.axis.x-mapped.y*q.axis.y,mapped.y*q.axis.x+mapped.x*q.axis.y);
    let s=phaseEllipseSlope(orbit,axis);let n=normalize(vec3f(-s.x,1.0,-s.y));
    let qp=q.metric*2.0*q.low/denominator;let weight=qp/phaseQuadratic(s,p);
    result+=phaseNumerator(s,p)*weight*weight*denominator/(1.0+ratio)*shadow(world,n);
  }
  return q.integral*result/f32(nodes)*g.sunlight.xyz*g.sun.w*waterSunTransmission(world);
}
fn waterWarpedSunInterval(world:vec3f,orbit:PhaseEllipse,p:PhaseGGX,q:PhaseWarp,nodes:u32,shift:f32,start:f32,width:f32)->vec3f {
  let ratio=q.low/q.high;var result=vec3f(0.0);
  let interval=phaseWarpInterval(start,width,q);
  for(var i=0u;i<nodes;i++) {
    let angle=interval.x+interval.y*(f32(i)+shift)/f32(nodes);let cs=vec2f(cos(angle),sin(angle));
    let denominator=1.0+cs.x+ratio*(1.0-cs.x);
    let mapped=vec2f(1.0+cs.x-ratio*(1.0-cs.x),2.0*sqrt(ratio)*cs.y)/denominator;
    let axis=vec2f(mapped.x*q.axis.x-mapped.y*q.axis.y,mapped.y*q.axis.x+mapped.x*q.axis.y);
    let s=phaseEllipseSlope(orbit,axis);let n=normalize(vec3f(-s.x,1.0,-s.y));
    let qp=q.metric*2.0*q.low/denominator;let weight=qp/phaseQuadratic(s,p);
    result+=phaseNumerator(s,p)*weight*weight*denominator/(1.0+ratio)*shadow(world,n);
  }
  return q.integral*result/f32(nodes)*g.sunlight.xyz*g.sun.w*waterSunTransmission(world)*interval.y/width;
}
struct WaterConditionalResult { value:mat2x3f,accepted:u32,path:u32 };
// The remainder is explicitly sampled at its authored start; complete periods
// are never substituted for a partial orbit. Finite Fourier factors do the same
// splitting algebraically without discarding the phase at either endpoint.
fn waterConditionalResponse(world:vec3f,orbit:PhaseEllipse,width:f32,base:vec3f,rough:f32,metallic:f32,quality:u32)->WaterConditionalResult {
  var result:WaterConditionalResult;let periods=floor(width/BRDF_TAU);
  if(periods<1.0) {return result;}
  let view=normalize(g.camera.xyz-world);
  // Broad smooth response: a fixed 16-sample fit followed by separate off-grid
  // source checks. These are measured query checks, not source-wide certificates.
  if(rough>=0.16&&view.y>0.15) {
    let program=fitWaterPhasePolynomial(world,orbit,base,rough,metallic);
    var error=0.0;var scale=0.02;
    for(var i=0u;i<4u;i++) {
      let angle=BRDF_TAU*(f32(i)+0.31415927)/4.0;
      let actual=waterEllipseResponse(world,orbit,angle,base,rough,metallic);
      let predicted=evaluateWaterPhasePolynomial(program,angle,0.0);
      error=max(error,waterRadianceLength(actual-predicted));scale=max(scale,waterRadianceLength(actual));
    }
    if(error<=scale*select(0.01,0.005,quality>=3u)) {
      result.value=waterRadiancePositive(evaluateWaterPhasePolynomial(program,0.0,width));result.accepted=1u;result.path=2u;return result;
    }
  }
  // The warp is for a sharp sun highlight. The smooth response is integrated
  // separately, while sun visibility stays inside every warped evaluation.
  if(rough<=0.12&&view.y>0.1&&g.sun.y>0.05&&dot(view+g.sun.xyz,view+g.sun.xyz)>1e-8) {
    let p=preparePhaseGGX(view,g.sun.xyz,rough,vec3f(0.02037));
    let q=preparePhaseWarp(orbit,p);
    if(q.valid==1u) {
      let nodes=select(4u,8u,quality>=2u);
      let warped=waterWarpedSun(world,orbit,p,q,nodes,0.5);
      // A distinct 12-node lattice validates this query, including its shadow
      // discontinuities; 4/8 agreement alone is deliberately not the criterion.
      let check=waterWarpedSun(world,orbit,p,q,12u,0.27182818);
      let tolerance=select(0.015,0.0075,quality>=3u);
      if(length(warped-check)<=tolerance*max(length(check),0.02)) {
        var full=waterLighting(warped);
        for(var i=0u;i<8u;i++) {
          let angle=BRDF_TAU*(f32(i)+0.5)/8.0;
          full+=waterNonSunResponseAtSlope(world,phaseEllipseSlope(orbit,vec2f(cos(angle),sin(angle))),base,rough,metallic)*(1.0/8.0);
        }
        let remainder=width-periods*BRDF_TAU;var tail=mat2x3f();
        // Start at -width/2 + full periods; removing full periods preserves phase.
        if(remainder>1e-6) {
          let start=-width*0.5;
          for(var i=0u;i<16u;i++) {let angle=start+remainder*(f32(i)+0.5)/16.0;tail+=waterNonSunResponseAtSlope(world,phaseEllipseSlope(orbit,vec2f(cos(angle),sin(angle))),base,rough,metallic)*(1.0/16.0);}
        }
        if(remainder>1e-6) {
          let tailSun=waterWarpedSunInterval(world,orbit,p,q,16u,0.5,-width*0.5,remainder);
          let tailCheck=waterWarpedSunInterval(world,orbit,p,q,24u,0.31415927,-width*0.5,remainder);
          if(length(tailSun-tailCheck)>tolerance*max(length(tailCheck),0.02)) {return result;}
          tail+=waterLighting(tailSun);
        }
        result.value=(full*(periods*BRDF_TAU)+tail*remainder)*(1.0/width);result.accepted=1u;result.path=1u;return result;
      }
    }
  }
  return result;
}
// The slope-to-unit-normal map is 1-Lipschitz in angular distance, so the
// authored slope excursion also bounds the normal cone. A lobe which cannot
// enter that cone is broad locally even when the material itself is sharp.
fn waterLightFeatureWidth(n:vec3f,view:vec3f,light:vec3f,cone:f32,rough:f32)->f32 {
  let sum=view+light;if(dot(sum,sum)<1e-12) {return 1.0;}
  if(dot(n,light)+cone<=0.0) {return 1.0;}
  let halfAngle=acos(clamp(dot(n,normalize(sum)),-1.0,1.0));
  let minimum=max(0.0,halfAngle-cone);let sine=sin(min(minimum,BRDF_PI*0.5));
  let a2=rough*rough*rough*rough;
  return min(sqrt(a2+(1.0-a2)*sine*sine),max(0.05,dot(n,light)-cone));
}
fn waterFeatureWidthAtSlope(world:vec3f,slope:vec2f,cone:f32,rough:f32)->f32 {
  let n=normalize(vec3f(-slope.x,1.0,-slope.y));
  let view=normalize(g.camera.xyz-world);
  var width=min(waterLightFeatureWidth(n,view,g.sun.xyz,cone,rough),max(0.05,dot(n,view)-cone));
  for(var i=0u;i<min(u32(g.viewport.z),8u);i++) {
    let offset=g.points[i].position.xyz-world;let distanceSquared=max(dot(offset,offset),0.01);
    if(g.points[i].position.w*length(g.points[i].color.xyz)/(1.0+distanceSquared)>0.001) {
      width=min(width,waterLightFeatureWidth(n,view,offset*inverseSqrt(distanceSquared),cone,rough));
    }
  }
  // Sky-table horizon structure is independent of the GGX roughness.
  if(abs(reflect(-view,n).y)<=0.05+2.0*cone) {width=min(width,0.025);}
  return max(width,rough*rough*0.01);
}
fn waterLocalFeatureWidth(world:vec3f,box:WaterPhaseBox,cone:f32,rough:f32)->f32 {
  return waterFeatureWidthAtSlope(world,waterBoxSlope(box,vec3f(0.0)),cone,rough);
}
// Low-order Gauss rules integrate the locally smooth response more accurately
// than midpoint nodes for the same 2/4 samples. Dense reference stays independent.
fn waterQuadratureNode(index:u32,count:u32)->vec2f {
  if(count==1u) {return vec2f(0.0,1.0);}
  if(count==2u) {return vec2f(select(-0.288675134595,0.288675134595,index==1u),0.5);}
  if(count==4u) {
    let positions=array<f32,4>(-0.430568155797,-0.169990521792,0.169990521792,0.430568155797);
    let weights=array<f32,4>(0.173927422569,0.326072577431,0.326072577431,0.173927422569);
    return vec2f(positions[index],weights[index]);
  }
  return vec2f((f32(index)+0.5)/f32(count)-0.5,1.0/f32(count));
}
fn waterRegularResponse(world:vec3f,dx:vec3f,dy:vec3f,box:WaterPhaseBox,base:vec3f,rough:f32,metallic:f32,quality:u32)->mat2x3f {
  var variation=vec3f(0.0);var excursion=vec3f(0.0);
  for(var i=0u;i<box.count;i++) {
    let change=abs(vec3f(box.dx[i],box.dy[i],box.dt[i]));
    variation+=length(box.slopes[i])*min(change*0.5,vec3f(2.0));excursion=max(excursion,change);
  }
  var width=max(rough*rough,0.0004);
  if(!WATER_COHERENT&&!WATER_REFERENCE) {width=waterLocalFeatureWidth(world,box,variation.x+variation.y+variation.z,rough);}
  var counts=vec3u(2u);
  for(var axis=0u;axis<3u;axis++) {
    if(variation[axis]<=0.02*width) {counts[axis]=1u;}
    else if(variation[axis]>0.5*width&&quality>=2u) {counts[axis]=4u;}
    if(variation[axis]>2.0*width&&quality>=3u) {counts[axis]=8u;}
  }
  if(excursion.z>0.2) {counts.z=min(select(128u,256u,quality>=2u),max(4u,u32(ceil(excursion.z*select(8.0,48.0,rough<=0.12)))));}
  if(WATER_REFERENCE) {
    counts=vec3u(16u,16u,select(1u,8u,box.shutter>0.0));
  }
  if(excursion.x==0.0) {counts.x=1u;}if(excursion.y==0.0) {counts.y=1u;}if(excursion.z==0.0) {counts.z=1u;}
  if(!WATER_REFERENCE&&!WATER_COHERENT) {
    // Allocate a fixed whole-query budget, not independent per-axis budgets
    // whose product can silently grow to hundreds of shader evaluations.
    let requested=counts;counts=vec3u(1u);
    let budget=select(select(4u,8u,quality>=2u),16u,quality>=3u);
    for(var step=0u;step<4u;step++) {
      if(counts.x*counts.y*counts.z*2u>budget) {break;}
      var best=3u;var score=-1.0;
      for(var axis=0u;axis<3u;axis++) {
        let priority=variation[axis]/f32(counts[axis]*counts[axis]);
        if(counts[axis]<requested[axis]&&priority>score) {best=axis;score=priority;}
      }
      if(best==3u) {break;}counts[best]*=2u;
    }
  }
  if(WATER_COHERENT&&!WATER_REFERENCE&&counts.x*counts.y*counts.z>256u) {
    let requested=counts;counts=vec3u(1u);
    for(var step=0u;step<8u;step++) {
      if(counts.x*counts.y*counts.z*2u>256u) {break;}
      var best=3u;var score=-1.0;
      for(var axis=0u;axis<3u;axis++) {
        let priority=variation[axis]/f32(counts[axis]*counts[axis]);
        if(counts[axis]<requested[axis]&&priority>score) {best=axis;score=priority;}
      }
      if(best==3u) {break;}counts[best]*=2u;
    }
  }
  var sum=mat2x3f();
  // Spatial position is constant through the inner shutter loop, allowing exact
  // reuse of atmospheric sun transmission while normals and shadows still vary.
  for(var y=0u;y<counts.y;y++) {for(var x=0u;x<counts.x;x++) {for(var t=0u;t<counts.z;t++) {
    var sx=waterQuadratureNode(x,counts.x);var sy=waterQuadratureNode(y,counts.y);var st=waterQuadratureNode(t,counts.z);
    if(WATER_REFERENCE) {
      sx=vec2f((f32(x)+0.5)/f32(counts.x)-0.5,1.0/f32(counts.x));
      sy=vec2f((f32(y)+0.5)/f32(counts.y)-0.5,1.0/f32(counts.y));
      st=vec2f((f32(t)+0.5)/f32(counts.z)-0.5,1.0/f32(counts.z));
    }
    let offset=vec3f(sx.x,sy.x,st.x);let sample=world+dx*offset.x+dy*offset.y;
    sum+=waterResponseAtSlope(sample,waterBoxSlope(box,offset),base,rough,metallic)*(sx.y*sy.y*st.y);
  }}}
  return sum;
}
// Ordinary water deliberately has no phase arrays. Its compact vector summary
// survives register allocation without the coherent program's large value copies.
struct WaterFootprintSummary { slope:vec2f,variation:vec3f,excursion:vec3f };
fn waterFootprintSummary(world:vec3f,dx:vec3f,dy:vec3f,shutter:f32)->WaterFootprintSummary {
  var summary:WaterFootprintSummary;
  for(var i=0u;i<min(u32(obj.flags.w),8u);i++) {
    let wave=obj.waves[i];let k=BRDF_TAU/wave.shape.y;
    let carrier=k*vec2f(cos(wave.shape.w),sin(wave.shape.w));let slope=wave.shape.x*carrier;
    let phase=dot(carrier,world.xz)-k*wave.shape.z*g.params.x+wave.phase.x;
    let rates=abs(vec3f(dot(carrier,dx.xz),dot(carrier,dy.xz),-k*wave.shape.z*shutter));
    summary.slope+=slope*cos(phase);
    summary.variation+=length(slope)*min(rates*0.5,vec3f(2.0));
    summary.excursion=max(summary.excursion,rates);
  }
  return summary;
}
fn waterOrdinarySample(world:vec3f,dx:vec3f,dy:vec3f,shutter:f32,offset:vec3f,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  let sample=world+dx*offset.x+dy*offset.y;
  let slope=authoredWaterSlope(sample.xz,g.params.x+shutter*offset.z);
  return waterResponseAtSlope(sample,slope,base,rough,metallic);
}
// A hard total response budget includes every spatial and shutter dimension.
// Scalar cases avoid dynamically indexed constant arrays becoming private memory.
fn waterDenseGaussNode(index:u32,count:u32)->vec2f {
  if(count<=4u) {return waterQuadratureNode(index,count);}
  let positive=min(index,count-1u-index);var node=vec2f(0.0);
  if(count==8u) {switch positive {
    case 0u: {node=vec2f(0.4801449282,0.05061426815);}
    case 1u: {node=vec2f(0.3983332387,0.1111905172);}
    case 2u: {node=vec2f(0.262766205,0.1568533229);}
    case 3u: {node=vec2f(0.09171732125,0.1813418917);}
    default: {}
  }}
  if(count==16u) {switch positive {
    case 0u: {node=vec2f(0.4947004675,0.01357622971);}
    case 1u: {node=vec2f(0.4722875115,0.03112676197);}
    case 2u: {node=vec2f(0.4328156012,0.04757925584);}
    case 3u: {node=vec2f(0.3777022042,0.06231448563);}
    case 4u: {node=vec2f(0.3089381222,0.07479799441);}
    case 5u: {node=vec2f(0.2290083888,0.0845782597);}
    case 6u: {node=vec2f(0.1408017754,0.09130170752);}
    case 7u: {node=vec2f(0.04750625492,0.09472530523);}
    default: {}
  }}
  if(count==32u) {switch positive {
    case 0u: {node=vec2f(0.4986319309,0.003509305005);}
    case 1u: {node=vec2f(0.4928057558,0.008137197365);}
    case 2u: {node=vec2f(0.4823811278,0.01269603265);}
    case 3u: {node=vec2f(0.467453038,0.01713693146);}
    case 4u: {node=vec2f(0.4481605779,0.02141794901);}
    case 5u: {node=vec2f(0.4246838069,0.02549902963);}
    case 6u: {node=vec2f(0.397241898,0.02934204674);}
    case 7u: {node=vec2f(0.3660910594,0.03291111139);}
    case 8u: {node=vec2f(0.3315221335,0.03617289705);}
    case 9u: {node=vec2f(0.2938578786,0.03909694789);}
    case 10u: {node=vec2f(0.2534499545,0.04165596211);}
    case 11u: {node=vec2f(0.2106756381,0.0438260465);}
    case 12u: {node=vec2f(0.1659343011,0.04558693935);}
    case 13u: {node=vec2f(0.1196436811,0.04692219954);}
    case 14u: {node=vec2f(0.07223598079,0.04781936004);}
    case 15u: {node=vec2f(0.02415383284,0.04827004426);}
    default: {}
  }}
  return vec2f(select(-node.x,node.x,index<count/2u),node.y);
}
fn waterDenseTensor(world:vec3f,dx:vec3f,dy:vec3f,shutter:f32,base:vec3f,rough:f32,metallic:f32,counts:vec3u)->mat2x3f {
  var result=mat2x3f();
  let total=counts.x*counts.y*counts.z;
  // The explicit flattened limit also protects this expensive loop if a caller
  // ever violates the allocator's invariant. Production never submits a larger box.
  for(var sample=0u;sample<WATER_MAX_HIGHLIGHT_SAMPLES;sample++) {
    if(sample>=total) {break;}
    let x=sample%counts.x;let y=(sample/counts.x)%counts.y;let t=sample/(counts.x*counts.y);
    let sx=waterDenseGaussNode(x,counts.x);let sy=waterDenseGaussNode(y,counts.y);let st=waterDenseGaussNode(t,counts.z);
    result+=waterOrdinarySample(world,dx,dy,shutter,vec3f(sx.x,sy.x,st.x),base,rough,metallic)*(sx.y*sy.y*st.y);
  }
  return result;
}
fn waterHalfVectorVariation(world:vec3f,dx:vec3f,dy:vec3f,light:vec3f,lightPosition:vec3f,point:bool)->vec3f {
  let viewOffset=g.camera.xyz-world;let distance=length(viewOffset);let view=viewOffset/max(distance,0.0001);
  let extents=vec3f(length(dx)*0.5,length(dy)*0.5,0.0);let extent=extents.x+extents.y;
  var changes=extents/max(distance-extent,0.0001);
  if(point) {changes+=extents/max(length(lightPosition-world)-extent,0.0001);}
  return changes/max(length(view+light)-changes.x-changes.y,0.0001);
}
fn waterDenseSlopeVariation(world:vec3f,dx:vec3f,dy:vec3f,shutter:f32)->vec3f {
  var variation=vec3f(0.0);
  for(var i=0u;i<min(u32(obj.flags.w),8u);i++) {
    let wave=obj.waves[i];let k=BRDF_TAU/wave.shape.y;let carrier=k*vec2f(cos(wave.shape.w),sin(wave.shape.w));
    let phase=dot(carrier,world.xz)-k*wave.shape.z*g.params.x+wave.phase.x;
    let delta=0.5*abs(vec3f(dot(carrier,dx.xz),dot(carrier,dy.xz),-k*wave.shape.z*shutter));
    // Taylor's cosine remainder gives a tighter real bound at a stationary wave
    // without collapsing the original carrier or losing its phase correlation.
    variation+=abs(wave.shape.x)*length(carrier)*min(min(delta,abs(sin(phase))*delta+0.5*delta*delta),vec3f(2.0));
  }
  return variation;
}
fn waterSharpFootprint(world:vec3f,dx:vec3f,dy:vec3f,slope:vec2f,normalCone:f32,rough:f32)->bool {
  let view=normalize(g.camera.xyz-world);let n=normalize(vec3f(-slope.x,1.0,-slope.y));
  let motion=waterHalfVectorVariation(world,dx,dy,g.sun.xyz,vec3f(0.0),false);
  let cone=normalCone+motion.x+motion.y;
  let sum=view+g.sun.xyz;
  if(dot(n,g.sun.xyz)+normalCone>0.0&&dot(sum,sum)>1e-12&&acos(clamp(dot(n,normalize(sum)),-1.0,1.0))<=cone+4.0*rough*rough) {return true;}
  for(var i=0u;i<min(u32(g.viewport.z),8u);i++) {
    let offset=g.points[i].position.xyz-world;let lengthSquared=max(dot(offset,offset),0.01);let light=offset*inverseSqrt(lengthSquared);
    if(g.points[i].position.w*length(g.points[i].color.xyz)/(1.0+lengthSquared)<=0.001) {continue;}
    let pointMotion=waterHalfVectorVariation(world,dx,dy,light,g.points[i].position.xyz,true);
    let pointSum=view+light;
    if(dot(n,light)+normalCone>0.0&&dot(pointSum,pointSum)>1e-12&&acos(clamp(dot(n,normalize(pointSum)),-1.0,1.0))<=normalCone+pointMotion.x+pointMotion.y+4.0*rough*rough) {return true;}
  }
  return false;
}
fn waterBoundedHighlight(world:vec3f,dx:vec3f,dy:vec3f,shutter:f32,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  let slopeVariation=waterDenseSlopeVariation(world,dx,dy,shutter);
  var variation=slopeVariation+waterHalfVectorVariation(world,dx,dy,g.sun.xyz,vec3f(0.0),false);
  for(var i=0u;i<min(u32(g.viewport.z),8u);i++) {
    let offset=g.points[i].position.xyz-world;let squared=max(dot(offset,offset),0.01);
    if(g.points[i].position.w*length(g.points[i].color.xyz)/(1.0+squared)>0.001) {
      variation=max(variation,slopeVariation+waterHalfVectorVariation(world,dx,dy,offset*inverseSqrt(squared),g.points[i].position.xyz,true));
    }
  }
  // Favor axes with the largest squared angular change under one total budget.
  // Saturation is an estimate, not a convergence proof; never expand all axes.
  let counts=waterHighlightCounts(variation,rough);
  return waterDenseTensor(world,dx,dy,shutter,base,rough,metallic,counts);
}
fn waterOrdinaryResponse(world:vec3f,dx:vec3f,dy:vec3f,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  let quality=clamp(u32(max(obj.waves[0].phase.z,1.0)),1u,3u);
  let mode=u32(obj.waves[1].phase.z);let shutter=max(obj.waves[1].phase.y,0.0);
  if(mode==2u) {return waterResponseAtSlope(world,authoredWaterSlope(world.xz,g.params.x),base,rough,metallic);}
  let summary=waterFootprintSummary(world,dx,dy,shutter);
  let cone=summary.variation.x+summary.variation.y+summary.variation.z;
  let width=waterFeatureWidthAtSlope(world,summary.slope,cone,rough);
  let halfMotion=waterHalfVectorVariation(world,dx,dy,g.sun.xyz,vec3f(0.0),false);
  if(rough<=0.25&&cone+halfMotion.x+halfMotion.y>0.03*rough*rough&&waterSharpFootprint(world,dx,dy,summary.slope,cone,rough)) {
    return waterBoundedHighlight(world,dx,dy,shutter,base,rough,metallic);
  }
  if(mode!=3u) {
    if(cone<=select(0.05,0.03,quality>=3u)*width) {return waterResponseAtSlope(world,summary.slope,base,rough,metallic);}
    if(cone<=0.15*width) {
      let center=waterResponseAtSlope(world,summary.slope,base,rough,metallic);var checked=mat2x3f();
      for(var i=0u;i<4u;i++) {
        let sx=select(-1.0,1.0,(i&1u)!=0u);let sy=select(-1.0,1.0,(i&2u)!=0u);
        checked+=waterOrdinarySample(world,dx,dy,shutter,vec3f(sx,sy,sx*sy)*0.288675134595,base,rough,metallic)*0.25;
      }
      if(waterRadianceLength(center-checked)<=0.005*max(waterRadianceLength(checked),0.02)) {return center;}
    }
  }
  var requested=vec3u(2u);
  for(var axis=0u;axis<3u;axis++) {
    if(summary.variation[axis]<=0.02*width) {requested[axis]=1u;}
    else if(summary.variation[axis]>0.5*width&&quality>=2u) {requested[axis]=4u;}
    if(summary.variation[axis]>2.0*width&&quality>=3u) {requested[axis]=8u;}
  }
  if(summary.excursion.z>0.2) {requested.z=min(select(128u,256u,quality>=2u),max(4u,u32(ceil(summary.excursion.z*select(8.0,48.0,rough<=0.12)))));}
  if(summary.excursion.x==0.0) {requested.x=1u;}if(summary.excursion.y==0.0) {requested.y=1u;}if(summary.excursion.z==0.0) {requested.z=1u;}
  var counts=vec3u(1u);let budget=select(select(4u,8u,quality>=2u),16u,quality>=3u);
  for(var step=0u;step<4u;step++) {
    if(counts.x*counts.y*counts.z*2u>budget) {break;}
    var best=3u;var score=-1.0;
    for(var axis=0u;axis<3u;axis++) {
      let priority=summary.variation[axis]/f32(counts[axis]*counts[axis]);
      if(counts[axis]<requested[axis]&&priority>score) {best=axis;score=priority;}
    }
    if(best==3u) {break;}counts[best]*=2u;
  }
  var result=mat2x3f();
  for(var y=0u;y<counts.y;y++) {for(var x=0u;x<counts.x;x++) {for(var t=0u;t<counts.z;t++) {
    let sx=waterQuadratureNode(x,counts.x);let sy=waterQuadratureNode(y,counts.y);let st=waterQuadratureNode(t,counts.z);
    result+=waterOrdinarySample(world,dx,dy,shutter,vec3f(sx.x,sy.x,st.x),base,rough,metallic)*(sx.y*sy.y*st.y);
  }}}
  return result;
}
fn integratedWaterMicroResponse(world:vec3f,dx:vec3f,dy:vec3f,base:vec3f,rough:f32,metallic:f32)->mat2x3f {
  if(!WATER_REFERENCE){let compiled=compiledWaterResponse(world,dx,dy,base,rough,metallic);if(compiled.valid){return compiled.value;}}
  if(!WATER_COHERENT&&!WATER_REFERENCE) {return waterOrdinaryResponse(world,dx,dy,base,rough,metallic);}
  let quality=clamp(u32(max(obj.waves[0].phase.z,1.0)),1u,3u);
  let box=prepareWaterPhaseBox(world,dx,dy);
  let mode=u32(obj.waves[1].phase.z);
  if(WATER_REFERENCE) {return waterRegularResponse(world,dx,dy,box,base,rough,metallic,4u);}
  if(mode==2u) {return waterResponseAtSlope(world,waterBoxSlope(box,vec3f(0.0)),base,rough,metallic);}
  if(mode==3u) {return waterRegularResponse(world,dx,dy,box,base,rough,metallic,quality);}
  var slopeVariation=0.0;
  for(var i=0u;i<box.count;i++) {
    let excursion=abs(box.dx[i])+abs(box.dy[i])+abs(box.dt[i]);
    slopeVariation+=length(box.slopes[i])*min(excursion*0.5,2.0);
  }
  var featureWidth=rough*rough;
  if(!WATER_COHERENT) {featureWidth=waterLocalFeatureWidth(world,box,slopeVariation,rough);}
  let resolvedTolerance=select(0.05,0.03,quality>=3u);
  if(slopeVariation<=resolvedTolerance*featureWidth) {return waterResponseAtSlope(world,waterBoxSlope(box,vec3f(0.0)),base,rough,metallic);}
  if(!WATER_COHERENT&&slopeVariation<=0.15*featureWidth) {
    let center=waterResponseAtSlope(world,waterBoxSlope(box,vec3f(0.0)),base,rough,metallic);
    var checked=mat2x3f();
    // Four symmetric nodes preserve all pairwise pixel/shutter correlations;
    // compare the complete response, including the sky and shadow, before reuse.
    for(var i=0u;i<4u;i++) {
      let sx=select(-1.0,1.0,(i&1u)!=0u);let sy=select(-1.0,1.0,(i&2u)!=0u);
      let offset=vec3f(sx,sy,sx*sy)*0.288675134595;
      checked+=waterResponseAtSlope(world+dx*offset.x+dy*offset.y,waterBoxSlope(box,offset),base,rough,metallic)*0.25;
    }
    if(waterRadianceLength(center-checked)<=0.005*max(waterRadianceLength(checked),0.02)) {return center;}
  }
  if(WATER_COHERENT) {
  var axis=3u;var rate=0.0;
  for(var candidate=0u;candidate<3u;candidate++) {
    let carrierRate=waterCommonRate(box,candidate);
    let spatialExtent=select(select(length(dx),length(dy),candidate==1u),0.0,candidate==2u);
    // Local view/transport freezing is restricted to one percent of eye distance.
    if(abs(carrierRate)>=BRDF_TAU&&abs(carrierRate)>abs(rate)&&spatialExtent<=0.01*max(distance(world,g.camera.xyz),1.0)) {axis=candidate;rate=carrierRate;}
  }
  if(axis<3u) {
    var sum=mat2x3f();var accepted=true;
    var transverseExcursion=0.0;
    for(var i=0u;i<box.count;i++) {
      if(axis!=0u) {transverseExcursion=max(transverseExcursion,abs(box.dx[i]));}
      if(axis!=1u) {transverseExcursion=max(transverseExcursion,abs(box.dy[i]));}
      if(axis!=2u) {transverseExcursion=max(transverseExcursion,abs(box.dt[i]));}
    }
    let outerNodes=select(select(2u,4u,quality>=2u),1u,transverseExcursion==0.0);
    for(var i=0u;i<outerNodes;i++) {
      let u=(f32(i)+0.5)/f32(outerNodes)-0.5;
      let v=fract((f32(i)+0.5)*0.618033989)-0.5;
      var offset=vec3f(0.0,u,v);
      if(axis==1u) {offset=vec3f(u,0.0,v);}if(axis==2u) {offset=vec3f(u,v,0.0);}
      // A zero shutter has exactly one temporal state, not a second phase axis.
      if(box.shutter==0.0) {offset.z=0.0;}
      let sample=world+dx*offset.x+dy*offset.y;
      let orbit=waterConditionalOrbit(box,axis,offset);
      let response=waterConditionalResponse(sample,orbit,abs(rate),base,rough,metallic,quality);
      accepted=accepted&&response.accepted==1u;sum+=response.value;
    }
    if(accepted) {return sum*(1.0/f32(outerNodes));}
  }
  }
  return waterRegularResponse(world,dx,dy,box,base,rough,metallic,quality);
}

// Transport traces are paid once per macroSample pixel. Micro sky Fresnel/reflection,
// sun and point-light response stay inside the correlated appearance integral.
fn integratedWaterResponse(world:vec3f,dx:vec3f,dy:vec3f,base:vec3f,rough:f32,metallic:f32)->vec3f {
  let macroSample=waterSampleFiltered(world.xz,obj.waves[0].phase.y);
  let n=normalize(vec3f(-macroSample.y,1.0,-macroSample.z));
  let view=normalize(g.camera.xyz-world);
  let response=integratedWaterMicroResponse(world,dx,dy,base,rough,metallic);
  return waterTransportWithSky(world,n,view,base,rough,response[0])+response[1];
}
