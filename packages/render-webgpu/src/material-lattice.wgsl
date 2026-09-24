override MATERIAL_CACHE:bool=true;
@group(0) @binding(13) var materialLattice:texture_3d<f32>;
@group(0) @binding(14) var<uniform> materialLatticeBounds:vec4i;
fn cachedMaterialNoise(p:vec3f)->f32 {
 if(materialLatticeBounds.w==0){return -1.0;}
 // The authored hash is periodic; preserve that domain across origin rebases.
 let cell=vec3i(floor(p));let wrapped=((cell+vec3i(512))%vec3i(1024)+vec3i(1024))%vec3i(1024)-vec3i(512);
 let q=wrapped-materialLatticeBounds.xyz;
 if(any(q<vec3i(0))||any(q>=vec3i(materialLatticeBounds.w))){return -1.0;}
 let a=textureLoad(materialLattice,vec3i(q.xy,q.z*2),0);
 let b=textureLoad(materialLattice,vec3i(q.xy,q.z*2+1),0);
 let f=fract(p);let u=f*f*(3.0-2.0*f);
 return mix(mix(mix(a.x,a.y,u.x),mix(a.z,a.w,u.x),u.y),mix(mix(b.x,b.y,u.x),mix(b.z,b.w,u.x),u.y),u.z);
}
