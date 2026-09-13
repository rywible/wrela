// Authoritative localized nap source. Paired with localized_nap.py's numerical checks.
// Two continuous, seeded 3D cell fields replace the global periodic wave directions.
// No texture, geometry, extra draw, time input, or change to the sheen energy model.
float shortCoatHash(int3 cell,uint seed) {
    uint3 p=uint3(cell);
    uint h=p.x*0x8da6b343u ^ p.y*0xd8163841u ^ p.z*0xcb1ab31fu ^ seed;
    h^=h>>16;h*=0x7feb352du;h^=h>>15;h*=0x846ca68bu;h^=h>>16;
    return float(h>>8)*(2.f/16777215.f)-1.f;
}

float shortCoatValue(float3 p,uint seed) {
    int3 cell=int3(floor(p));
    float3 t=fract(p);
    // Quintic interpolation has matching value and first/second derivative at cell edges.
    t=t*t*t*(t*(t*6.f-15.f)+10.f);
    float z0=mix(mix(shortCoatHash(cell,seed),shortCoatHash(cell+int3(1,0,0),seed),t.x),
        mix(shortCoatHash(cell+int3(0,1,0),seed),shortCoatHash(cell+int3(1,1,0),seed),t.x),t.y);
    float z1=mix(mix(shortCoatHash(cell+int3(0,0,1),seed),shortCoatHash(cell+int3(1,0,1),seed),t.x),
        mix(shortCoatHash(cell+int3(0,1,1),seed),shortCoatHash(cell+int3(1,1,1),seed),t.x),t.y);
    return mix(z0,z1,t.z);
}

float3 shortCoatCoordinates(float3 p,uint layer) {
    return layer==0 ? float3(.8f*p.x+.6f*p.y,-.6f*p.x+.8f*p.y,p.z)*float3(145,145,62)
        : float3(-.6f*p.x+.8f*p.y,-.8f*p.x-.6f*p.y,p.z+.12f*p.x)*float3(239,239,103);
}

ShortCoatSample shortCoatSurface(float3 bindPosition,float3 dpdx,float3 dpdy,float wet) {
    float height=0;
    for(uint layer=0;layer<2;layer++) {
        float3 dx=shortCoatCoordinates(dpdx,layer),dy=shortCoatCoordinates(dpdy,layer);
        float footprint=max(length(dx),length(dy));
        // Conservative whole-cell fade, not an exact band limit: quintic value noise
        // has a spectral tail. The numerical comparison retains its residual explicitly.
        float filter=(1-smoothstep(.18f,.55f,footprint))
            *exp(-1.5f*(dot(dx,dx)+dot(dy,dy)));
        float3 coordinate=shortCoatCoordinates(bindPosition,layer)+float3(17.31f,-8.7f,41.6f)*float(layer);
        height+=shortCoatValue(coordinate,layer==0 ? 37011u:9973u)
            *(layer==0 ? .00016f:.00008f)*filter;
    }
    wet=saturate(wet);
    return {height*(1-.70f*wet),1+.018f*height/.00024f,.90f-.12f*wet};
}
