// Bounded, loop-free surface approximations. These are not volumetric skin,
// multiple-scattering fibers, refractive eyeballs, or a measured cloth BRDF.
fn creatureSafeDirection(value:vec3f,fallback:vec3f)->vec3f {
  let size=dot(value,value);
  return select(fallback,value*inverseSqrt(max(size,1e-12)),size>1e-12);
}
fn creatureCoordinates(local:vec3f)->vec3f {
  if(obj.creatureCoat.z<0.5) {return local;}
  let offset=local-obj.creatureOrigin.xyz;
  return vec3f(dot(offset,obj.creatureX.xyz),dot(offset,obj.creatureY.xyz),dot(offset,obj.creatureZ.xyz));
}
fn creatureTangent(n:vec3f,tangent:vec3f)->vec3f {
  let axis=select(vec3f(0,1,0),vec3f(1,0,0),abs(n.y)>0.9);
  let fallback=normalize(cross(axis,n));
  return creatureSafeDirection(tangent-n*dot(n,tangent),fallback);
}
// Anisotropic GGX in a deformed fiber frame, with Smith masking and Schlick Fresnel.
fn creatureFiberSpecular(n:vec3f,view:vec3f,light:vec3f,tangent:vec3f,rough:f32,f0:vec3f)->vec3f {
  let nv=dot(n,view);let nl=dot(n,light);let sum=view+light;
  if(nv<=0.0||nl<=0.0||dot(sum,sum)<1e-12) {return vec3f(0);}
  let t=creatureTangent(n,tangent);let b=cross(n,t);let h=normalize(sum);
  let aspect=sqrt(1.0-0.9*abs(obj.creatureFiber.w));
  let a=max(rough*rough,0.0036);
  let ax=select(a/aspect,a*aspect,obj.creatureFiber.w<0.0);
  let ay=select(a*aspect,a/aspect,obj.creatureFiber.w<0.0);
  let hx=dot(h,t)/ax;let hy=dot(h,b)/ay;let nh=dot(n,h);
  let q=hx*hx+hy*hy+nh*nh;
  let vx=ax*dot(view,t);let vy=ay*dot(view,b);
  let lx=ax*dot(light,t);let ly=ay*dot(light,b);
  let sv=sqrt(vx*vx+vy*vy+nv*nv);
  let sl=sqrt(lx*lx+ly*ly+nl*nl);
  let fresnel=f0+(vec3f(1)-f0)*pow(clamp(1.0-dot(view,h),0.0,1.0),5.0);
  return fresnel*nl/max(2.0*BRDF_PI*ax*ay*q*q*(nl*sv+nv*sl),1e-12);
}
// Includes incident cosine. Caller supplies visibility and irradiance so compiler
// finite-sun occlusion remains authoritative for diffuse illumination.
fn creatureDirect(base:vec3f,n:vec3f,view:vec3f,light:vec3f,tangent:vec3f,rough:f32,metallic:f32,irradiance:f32,visibility:f32)->vec3f {
  // Clay is the same lit, deformed geometry without any authored appearance response.
  if(g.params.z>9.5&&g.params.z<10.5) {
    return base*irradiance/BRDF_PI+brdfGGX(n,view,light,rough,vec3f(0.04))*visibility;
  }
  let family=obj.creature.x;
  let f0=mix(vec3f(0.04),base,metallic);
  var specular=brdfGGX(n,view,light,rough,f0)*visibility;
  let h=creatureSafeDirection(view+light,n);
  let fresnel=f0+(vec3f(1.0)-f0)*pow(clamp(1.0-dot(view,h),0.0,1.0),5.0);
  var diffuse=base*(1.0-metallic)*(vec3f(1.0)-fresnel)*irradiance/BRDF_PI;
  if(family>0.5&&family<1.5) {
    let nl=dot(n,light);
    if(obj.surface.x>2.5&&obj.surface.x<3.5) {
      let budget=foliageBudget(base,obj.creatureScatter.xyz,obj.creature.z,obj.creature.w);
      diffuse=foliageDiffuse(budget,nl,irradiance,visibility)*(1.0-metallic);
    } else {
    let wrapped=clamp((nl+0.35)/1.35,0.0,1.0)/1.35;
    let scatter=base*obj.creatureScatter.xyz*(1.0-metallic);
    diffuse=mix(diffuse,scatter*wrapped*visibility/BRDF_PI,obj.creature.y);
    let transmission=obj.creature.z*exp(-obj.creature.w/0.005)*max(-nl,0.0);
    diffuse+=scatter*transmission*visibility/BRDF_PI;
    }
  }
  if(family>1.5&&family<3.5) {
    if(family<2.5) {specular=creatureFiberSpecular(n,view,light,tangent,rough,f0)*visibility;}
    let grazing=pow(1.0-clamp(dot(n,view),0.0,1.0),5.0);
    let sheen=obj.creatureScatter.w*grazing;
    diffuse=diffuse*(1.0-sheen)+sqrt(max(base,vec3f(0)))*sheen*sqrt(max(irradiance,0.0))/BRDF_PI;
  }
  let coat=obj.creatureCoat.x;
  if(coat>0.0) {
    let coatF=0.04+0.96*pow(1.0-clamp(dot(n,view),0.0,1.0),5.0);
    let attenuation=1.0-coat*coatF;
    return (diffuse+specular)*attenuation+coat*brdfGGX(n,view,light,obj.creatureCoat.y,vec3f(0.04))*visibility;
  }
  return diffuse+specular;
}
