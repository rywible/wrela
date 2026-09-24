// Compiler-produced two-wave slope and inverse maps. Integrate the sharp GGX
// denominator by Green's theorem; never construct a lighting fit per pixel.
struct CompiledWaterResult { value:mat2x3f, valid:bool };
fn glintCross(a:vec2f,b:vec2f)->f32 {return a.x*b.y-a.y*b.x;}
fn glintMetric(s:vec2f,p:PhaseGGX)->vec2f {
 let a=sqrt(p.matrix.x);let b=p.matrix.y/a;
 return vec2f(a*s.x+b*s.y,sqrt(max(p.matrix.z-b*b,1e-12))*s.y);
}
fn glintRectangle(center:vec2f,dx:vec2f,dy:vec2f,delta:f32)->f32 {
 let determinant=glintCross(dx,dy);
 if(abs(determinant)<max(1e-16,1e-4*length(dx)*length(dy))){return -1.0;}
 let corners=array<vec2f,4>(center-0.5*dx-0.5*dy,center+0.5*dx-0.5*dy,center+0.5*dx+0.5*dy,center-0.5*dx+0.5*dy);
 var sum=0.0;
 for(var i=0u;i<4u;i++){
  let a=corners[i];let v=corners[(i+1u)%4u]-a;
  let A=dot(v,v);let B=dot(a,v);let cross=glintCross(a,v);
  let D=max(A*delta+cross*cross,1e-30);let root=sqrt(D);
  sum+=cross*atan2(A*root,D+B*(A+B))/root;
 }
 return sum/(2.0*delta*determinant);
}
fn glintPhase(world:vec3f,time:f32)->vec2f {
 return vec2f(dot(obj.glintWaves[0].zw,world.xz),dot(obj.glintWaves[1].zw,world.xz))+obj.glintTime.xy*time+vec2f(obj.waves[0].phase.x,obj.waves[1].phase.x);
}
fn glintSlope(phase:vec2f)->vec2f {return obj.glintWaves[0].xy*cos(phase.x)+obj.glintWaves[1].xy*cos(phase.y);}
fn glintSlopeDerivative(phase:vec2f,rate:vec2f)->vec2f {return -obj.glintWaves[0].xy*sin(phase.x)*rate.x-obj.glintWaves[1].xy*sin(phase.y)*rate.y;}
fn glintAnchor(phase:vec2f,dx:vec2f,dy:vec2f,desiredSlope:vec2f)->vec2f {
 let determinant=glintCross(dx,dy);if(abs(determinant)<1e-10){return vec2f(0.0);}
 let cosines=vec2f(dot(obj.glintInverse.xy,desiredSlope),dot(obj.glintInverse.zw,desiredSlope));
 if(any(abs(cosines)>=vec2f(0.999))){return vec2f(0.0);}
 let root=acos(cosines);var selected=vec2f(0.0);var best=0.75;
 for(var i=0u;i<4u;i++){
  let candidate=root*vec2f(select(-1.0,1.0,(i&1u)!=0u),select(-1.0,1.0,(i&2u)!=0u));
  let delta=candidate-phase;let wrapped=delta-floor((delta+BRDF_PI)/BRDF_TAU)*BRDF_TAU;
  let pixel=vec2f(glintCross(wrapped,dy),glintCross(dx,wrapped))/determinant;
  let distance=length(pixel);if(distance<best){best=distance;selected=pixel;}
 }
 return selected;
}
fn compiledWaterResponse(world:vec3f,dx:vec3f,dy:vec3f,base:vec3f,rough:f32,metallic:f32)->CompiledWaterResult {
 var result:CompiledWaterResult;result.valid=false;
 if(g.viewport.z>0.0||obj.glintTime.z<0.5||obj.waves[1].phase.z!=0.0||rough>0.12||rough<0.06){return result;}
 let view=normalize(g.camera.xyz-world);if(view.y<0.2||g.sun.y<0.1){return result;}
 let p=preparePhaseGGX(view,g.sun.xyz,rough,vec3f(0.02037));
 let ratesX=vec2f(dot(obj.glintWaves[0].zw,dx.xz),dot(obj.glintWaves[1].zw,dx.xz));
 let ratesY=vec2f(dot(obj.glintWaves[0].zw,dy.xz),dot(obj.glintWaves[1].zw,dy.xz));
 let extent=0.5*(abs(ratesX)+abs(ratesY));
 let alpha=rough*rough;let shutter=max(obj.waves[1].phase.y,0.0);
 let timeVariation=dot(vec2f(length(obj.glintWaves[0].xy),length(obj.glintWaves[1].xy)),abs(obj.glintTime.xy)*shutter);
 // Runtime domain checks bound source curvature and prevent temporal peaks from
 // escaping the two-node shutter integral. Unsupported regions keep the reference path.
 if(any(extent>vec2f(0.12))||timeVariation>alpha*0.75||length(dx)+length(dy)>0.08*alpha*distance(g.camera.xyz,world)){return result;}
 let middleSlope=glintSlope(glintPhase(world,g.params.x));
 // The local broad tail is already cheap in the ordinary source evaluator.
 if(phaseQuadratic(middleSlope,p)>p.delta*64.0){return result;}
 let visibility=waterGlintVisibility(world,dx,dy);if(visibility<0.0){return result;}
 var value=mat2x3f();
 for(var t=0u;t<2u;t++){
  let time=g.params.x+select(-0.288675134595,0.288675134595,t==1u)*shutter;
  let phase=glintPhase(world,time);let anchor=glintAnchor(phase,ratesX,ratesY,p.center);
  let anchorPhase=phase+ratesX*anchor.x+ratesY*anchor.y;
  let slope=glintSlope(anchorPhase);
  let sx=glintSlopeDerivative(anchorPhase,ratesX);let sy=glintSlopeDerivative(anchorPhase,ratesY);
  let expandedExtent=extent+abs(ratesX*anchor.x+ratesY*anchor.y);
  let remainder=0.5*dot(vec2f(length(obj.glintWaves[0].xy),length(obj.glintWaves[1].xy)),expandedExtent*expandedExtent);
  if(remainder>sqrt(p.delta)*0.001){return result;}
  let center=slope-sx*anchor.x-sy*anchor.y;
  let ix=glintMetric(sx,p);let iy=glintMetric(sy,p);let offset=glintMetric(center-p.center,p);
  // Avoid the cancellation-prone far-field or tiny-rectangle boundary formula.
  if(length(ix)+length(iy)<sqrt(p.delta)*0.1){return result;}
  let integral=glintRectangle(offset,ix,iy,p.delta);if(integral<=0.0){return result;}
  let numerator=phaseNumerator(slope,p);var broadResponse=mat2x3f();
  for(var i=0u;i<4u;i++){
   let u=select(-0.288675134595,0.288675134595,(i&1u)!=0u);let v=select(-0.288675134595,0.288675134595,(i&2u)!=0u);
   let position=world+dx*u+dy*v;let sampleSlope=glintSlope(phase+ratesX*u+ratesY*v);
   let candidate=phaseNumerator(sampleSlope,p);
   if(length(candidate-numerator)>0.005*max(length(numerator),1e-12)){return result;}
   broadResponse+=waterNonSunResponseAtSlope(position,sampleSlope,base,rough,metallic)*0.25;
  }
  value+=(broadResponse+waterLighting(numerator*integral*visibility*g.sunlight.xyz*g.sun.w*waterSunTransmission(world)))*0.5;
 }
 result.value=value;result.valid=true;return result;
}
