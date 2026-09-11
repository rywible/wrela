// Limestone layers and mineral variation, without vegetation or garden semantics.
void projectSurface(int kind,float3 local,float3 world,float3 n,thread float3 &color,thread float &rough,thread float &bump,thread float &strength) {
    if(kind==5) {
        float broad=filteredNoise(world.xz*.6+world.y*.27);
        float strata=sin(world.y*8+filteredNoise(world.xz*.35)*5);
        float detail=filteredNoise(world.xy*5+world.z);
        color*=.76+.3*broad+.055*strata;
        bump=detail*.0012+strata*.001;strength=1;rough=.88;
    }
}
