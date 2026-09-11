// Sanctuary surface recipes. The garden path belongs to this project.
void projectSurface(int kind,float3 local,float3 world,float3 n,thread float3 &color,thread float &rough,thread float &bump,thread float &strength) {
    float broad=filteredNoise(world.xz*.18),grain=filteredNoise(local.xz*18+local.y*3);
    if(kind==1) {
        float pathX=sin(world.z*.048)*9+sin(world.z*.105)*2;
        float path=1-smoothstep(1.5,2.7,abs(world.x-pathX)+(broad-.5)*.65);
        color=mix(float3(.29,.42,.16),float3(.42,.51,.23),broad);
        color=mix(color,float3(.53,.46,.30),path);color=mix(color,float3(.43,.45,.34),smoothstep(.18,.5,1-n.y));
        bump=filteredNoise(world.xz*14)*.0015+filteredNoise(world.xz*2)*.008;strength=1;rough=.95;
    } else if(kind==4) {
        float angle=atan2(local.z,local.x),ridge=sin(angle*17+filteredNoise(float2(local.y*.6,angle))*5);
        float footprint=max(fwidth(angle)*17,fwidth(local.y)*3);
        ridge*=1-smoothstep(.4,2.,footprint);
        bump=ridge*.009+filteredNoise(float2(angle*12,local.y*1.5))*.012;strength=1;
        color*=.7+.4*grain+.18*ridge;rough=.9;
    } else if(kind==5) {
        float stone=filteredNoise(local.xz*4+local.y),fine=filteredNoise(local.xy*35);
        float moss=smoothstep(.35,.8,n.y)*smoothstep(.35,.65,filteredNoise(world.xz*2));
        color=mix(color*(.72+.4*stone),float3(.28,.36,.13),moss*.65);
        bump=stone*.04+fine*.005;strength=1;rough=mix(.76,.98,moss);
    } else if(kind==6) {
        float angle=atan2(local.z,local.x),ribs=sin(angle*23+local.y*.7);
        ribs*=1-smoothstep(.4,2.,fwidth(angle)*23);
        float speck=filteredNoise(local.xz*45+local.y*9);
        color*=.78+.2*speck+.10*ribs;
        bump=(ribs*.007+grain*.003)*.01;strength=1;rough=.62+.13*grain;
    } else if(kind==2 || kind==8) {
        float leaves=filteredNoise(local.xy*9)+filteredNoise(local.xz*9);
        color*=.78+.2*leaves;bump=leaves*.012;strength=1;rough=.82;
    } else if(kind==9) {
        // Filtered short-coat structure, in bind space so the coat follows articulation.
        float coat=filteredNoise(local.xz*80+local.y*5);
        float clumps=filteredNoise(local.xz*9+local.y*4);
        color*=.95+.055*clumps+.015*coat;
        bump=coat*.00015+clumps*.0003;strength=1;rough=.9;
    } else if(kind==3) {color*=.85+.15*grain;rough=.85;}
}
