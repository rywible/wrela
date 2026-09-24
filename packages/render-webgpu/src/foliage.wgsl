// Empirical thin-leaf diffuse closure. The 5 mm attenuation distance is an
// artistic default, not a measured pigment extinction coefficient.
struct FoliageBudget { reflection:vec3f, transmission:vec3f };
fn foliageBudget(base:vec3f,scatter:vec3f,transmission:f32,thickness:f32)->FoliageBudget {
  let fraction=clamp(transmission,0.0,1.0)*exp(-max(thickness,0.0)/0.005);
  let albedo=clamp(base,vec3f(0),vec3f(1))*0.96;
  return FoliageBudget(albedo*(1.0-fraction),albedo*clamp(scatter,vec3f(0),vec3f(1))*fraction);
}
// Incident cosine included. R+T is bounded per color channel; specular is separate.
// A zero visibility produces exactly zero transmitted direct light.
fn foliageDiffuse(budget:FoliageBudget,nl:f32,frontIrradiance:f32,visibility:f32)->vec3f {
  return (budget.reflection*max(frontIrradiance,0.0)+budget.transmission*max(-nl,0.0)*clamp(visibility,0.0,1.0))/3.141592653589793;
}
