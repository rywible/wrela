export interface BranchGpuInput {
  geometry: number[];
  queries: number[];
  auxiliary: number[];
  reference: number[];
  plates: number;
  minimumRise: number;
}
export function branchShader(input: BranchGpuInput, compiled: boolean) {
  return `
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {
 if(id.x>=${input.queries.length / 8}u){return;}
 let p=input[id.x*2u];let q=input[id.x*2u+1u];let owner=u32(p.w);
 let d=vec3f(q.x,q.y+q.w*q.x,q.z);
 let eligible=${compiled ? "true" : "false"} && d.y>=${input.minimumRise}*length(d.xz)+0.000001;
 var begin=0u;var end=${input.plates}u;
 if(eligible){begin=u32(scalar(owner));end=u32(scalar(owner+1u));}
 var visible=1.0;
 for(var i=begin;i<end;i++){
   var j=i;if(eligible){j=u32(scalar(${input.plates + 1}u+i));}
   if(j==owner){continue;}
   let c=lights[j*4u];let u=lights[j*4u+1u];let v=lights[j*4u+2u];let n=cross(u.xyz,v.xyz);
   let den=dot(n,d);if(abs(den)<0.00000001){continue;}
   let t=dot(c.xyz-p.xyz,n)/den;if(t<=0.00001){continue;}
   let h=p.xyz+t*d-c.xyz;
   if(abs(dot(h,u.xyz))<=c.w && abs(dot(h,v.xyz))<=u.w){visible=0.0;break;}
 }
 output[id.x]=vec4f(visible,visible,visible,1);
}`;
}
