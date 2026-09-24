// Bounded isotropic multiple-scattering closure. The table contains mean diffuse
// radiance per unit solar irradiance, indexed by height and solar zenith cosine.
// Each ray integrates direct first-scattered sunlight and Lambertian ground;
// feedback is the transport of a unit isotropic field through the same medium.
// Passive scattering/albedo give 0 <= feedback <= 1. Sixteen nonnegative orders
// avoid the singular infinite geometric sum in an optically closed medium.
@group(0) @binding(0) var<uniform> atmosphereFrame:PhysicalAtmosphereUniform;
@group(0) @binding(1) var<storage,read> compiledAtmosphereTable:array<vec4f>;
@group(0) @binding(2) var diffuseOutput:texture_storage_2d<rgba16float,write>;
// Declared for unreachable cloud helpers; this builder never samples cloud noise.
@group(0) @binding(3) var physicalAtmosphereSampler:sampler;
fn physicalFrame()->PhysicalAtmosphereFrame { return atmosphereFrame.frame; }
fn physicalCloudForms()->vec4f {return atmosphereFrame.cloudForms;}
fn physicalCloudLayers()->vec4f {return atmosphereFrame.cloudLayers;}
fn physicalGrowth(index:u32,lobe:u32)->PhysicalCloudGrowth {return atmosphereFrame.cloudGrowth[index*6u+lobe];}
fn physicalFormation(index:u32)->PhysicalCloudFormation {return atmosphereFrame.formations[index];}

// This builder integrates direct transport explicitly and never samples itself.
fn physicalDiffuseRadiance(point:vec3f,sun:vec3f)->vec3f { return vec3f(0.0); }
const DIFFUSE_COSINES=array<f32,16>(-0.98940093499,-0.94457502307,-0.86563120239,-0.75540440836,-0.61787624440,-0.45801677766,-0.28160355078,-0.09501250984,0.09501250984,0.28160355078,0.45801677766,0.61787624440,0.75540440836,0.86563120239,0.94457502307,0.98940093499);
const DIFFUSE_WEIGHTS=array<f32,16>(0.02715245941,0.06225352394,0.09515851168,0.12462897126,0.14959598882,0.16915651940,0.18260341504,0.18945061046,0.18945061046,0.18260341504,0.16915651940,0.14959598882,0.12462897126,0.09515851168,0.06225352394,0.02715245941);
fn physicalFiniteScatteringSeries(first:vec3f,feedback:vec3f)->vec3f {
  var sum=first; var term=first;
  // The clamp only absorbs floating-point roundoff; physical feedback is <= 1.
  let passive=clamp(feedback,vec3f(0.0),vec3f(1.0));
  for(var order=1u;order<16u;order++) { term*=passive; sum+=term; }
  return sum;
}
@compute @workgroup_size(8,8) fn physicalDiffuseBuild(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(diffuseOutput);
  if(any(id.xy>=size)) { return; }
  let unit=vec2f(id.xy)/vec2f(size-vec2u(1u));
  let coordinate=unit.x*2.0-1.0; let cosine=sign(coordinate)*coordinate*coordinate;
  let sun=vec3f(sqrt(max(0.0,1.0-cosine*cosine)),cosine,0.0);
  let tangent=vec3f(sun.y,-sun.x,0.0);
  let height=compiledAtmosphereTable[0].y*pow(unit.y,4.0);
  let origin=vec3f(0.0,compiledAtmosphereTable[0].x+height,0.0);
  let groundAlbedo=clamp(atmosphereFrame.frame.ground.xyz,vec3f(0.0),vec3f(1.0));
  var first=vec3f(0.0); var feedback=vec3f(0.0);
  for(var polar=0u;polar<16u;polar++) {
    let mu=DIFFUSE_COSINES[polar]; let radial=sqrt(1.0-mu*mu);
    let rayleigh=physicalRayleighPhase(mu); let mie=physicalMiePhase(mu);
    for(var azimuth=0u;azimuth<8u;azimuth++) {
      let phi=2.0*ATMOSPHERE_PI*(f32(azimuth)+0.5)/8.0;
      let ray=sun*mu+radial*(tangent*cos(phi)+vec3f(0.0,0.0,1.0)*sin(phi));
      let path=physicalAtmospherePath(origin,ray);
      let nearScale=compiledAtmosphereTable[3].w/max(0.05,abs(ray.y));
      let logDistance=log(1.0+path.x/nearScale);
      var transmission=vec3f(1.0); var rayFirst=vec3f(0.0); var rayFeedback=vec3f(0.0);
      for(var step=0u;step<24u;step++) {
        let start=nearScale*(exp(logDistance*f32(step)/24.0)-1.0);
        let end=nearScale*(exp(logDistance*f32(step+1u)/24.0)-1.0);
        let point=origin+ray*((start+end)*0.5);
        let density=physicalDensity(length(point)-compiledAtmosphereTable[0].x);
        let extinction=physicalExtinction(density);
        let segment=transmission*physicalSegmentWeight(extinction,end-start);
        let molecular=compiledAtmosphereTable[1].xyz*density.x;
        let particles=vec3f(compiledAtmosphereTable[1].w*density.y+compiledAtmosphereTable[2].y*0.95*density.z);
        rayFirst+=segment*physicalSunTransmissionAt(point,sun)*(molecular*rayleigh+particles*mie);
        rayFeedback+=segment*(molecular+particles);
        transmission*=exp(-extinction*(end-start));
      }
      if(path.y>0.5) {
        let point=origin+ray*path.x;
        rayFirst+=transmission*groundAlbedo*physicalSunTransmissionAt(point,sun)*max(0.0,dot(normalize(point),sun))/ATMOSPHERE_PI;
        rayFeedback+=transmission*groundAlbedo;
      }
      // dOmega/(4*pi): Gauss-Legendre integrates cosine, eight azimuths.
      let weight=DIFFUSE_WEIGHTS[polar]/16.0;
      first+=rayFirst*weight; feedback+=rayFeedback*weight;
    }
  }
  textureStore(diffuseOutput,id.xy,vec4f(physicalFiniteScatteringSeries(first,feedback),1.0));
}
