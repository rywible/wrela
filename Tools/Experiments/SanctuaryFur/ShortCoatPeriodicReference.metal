// EXPERIMENT ONLY. Not included by any production shader or game.
// Pair with experiment.py's ShortCoatLUT.generated.metal (roughness 0.70).
// Seed37011, metres in immutable bind space. No opacity, geometry or global look change.
#include <metal_stdlib>
using namespace metal;
#include "ShortCoatLUT.generated.metal"

struct ShortCoatSample { float height; float albedoMultiplier; float roughness; };

ShortCoatSample shortCoatSurface(float3 bindPosition, float3 dpdx, float3 dpdy, float wet) {
    constexpr float4 modes[6] = {
        float4(14,5,11,.00010),float4(-19,7,13,.00008),
        float4(87,13,46,.000055),float4(-113,18,71,.000045),
        float4(157,23,-102,.000035),float4(-211,29,-134,.000025)
    };
    constexpr float phases[6] = {2.159548888f,3.642807707f,5.126066525f,
        .326140037f,1.809398855f,3.292657674f};
    float height=0;
    for(uint i=0;i<6;i++) {
        float2 footprint=float2(dot(modes[i].xyz,dpdx),dot(modes[i].xyz,dpdy));
        float cycles=max(abs(footprint.x),abs(footprint.y));
        float filter=(1-smoothstep(.30f,.50f,cycles))
            *exp(-39.478417604f*dot(footprint,footprint)/24);
        height+=modes[i].w*sin(6.283185307f*dot(modes[i].xyz,bindPosition)+phases[i])*filter;
    }
    wet=saturate(wet);
    return {height*(1-.70f*wet),1+.03f*height/.00034f,.90f-.12f*wet};
}

float shortCoatE(float nv) {
    float x=saturate(nv)*32;
    uint i=min(uint(x),31u);
    return mix(shortCoatEnergy[i],shortCoatEnergy[i+1],x-float(i));
}

// Charlie distribution + inexpensive Neubelt visibility. Fixed sheen roughness .70.
// baseDirect includes NoL, as the existing brdf() does. The caller retains actual
// sunlight/visibility; this function supplies no light and never changes exposure.
float3 shortCoatDirect(float3 baseDirect,float3 coatColor,float nv,float nl,float nh,float wet) {
    float amount=.16f*(1-.80f*saturate(wet));
    float inv=1/(.70f*.70f);
    float distribution=(2+inv)*pow(max(0.f,1-nh*nh),inv*.5f)/6.283185307f;
    float visibility=1/(4*max(nl+nv-nl*nv,1e-8f));
    float baseScale=max(0.f,1-amount*max(shortCoatE(nv),shortCoatE(nl)));
    return baseDirect*baseScale+coatColor*amount*distribution*visibility*max(0.f,nl);
}

// Isotropic-ambient approximation, not a sheen environment convolution or scene GI.
// ambientBase and ambientIrradiance must follow the host's existing radiometric convention.
float3 shortCoatAmbient(float3 ambientBase,float3 ambientIrradiance,float3 coatColor,float nv,float wet) {
    float weight=.16f*(1-.80f*saturate(wet))*shortCoatE(nv);
    return ambientBase*(1-weight)+ambientIrradiance*coatColor*weight;
}
