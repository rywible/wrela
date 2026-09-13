// Optional material-response hook; all reference recipes remain unchanged.
// Kind15 is an opt-in Sunhare study variant, never the default coat.
// Numerical sources: Tools/Experiments/SanctuaryFur/{experiment,localized_nap}.py.
#define PROJECT_COAT_RESPONSE 1
// Generated representation: experiment.py, seed37011, roughness.70.
constant float shortCoatEnergy[33] = {0.753864485f, 0.663104463f, 0.607632761f, 0.563828432f, 0.526769071f, 0.494293963f, 0.465205472f, 0.438752467f, 0.414425447f, 0.391859385f, 0.370781935f, 0.350983427f, 0.332298352f, 0.314593351f, 0.297759124f, 0.281704780f, 0.266353792f, 0.251641024f, 0.237510510f, 0.223913761f, 0.210808443f, 0.198157356f, 0.185927609f, 0.174089961f, 0.162618287f, 0.151489140f, 0.140681386f, 0.130175905f, 0.119955335f, 0.110003856f, 0.100307008f, 0.090851523f, 0.081624766f};
struct ShortCoatSample { float height; float albedoMultiplier; float roughness; };

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


float projectCoatCoverage(float3 local) { return smoothstep(.04f,.075f,local.y); }

bool projectWetFinish(int kind,float wet,thread float &rough,thread float &bump) {
    if(kind!=15)return false;
    bump*=1-.70f*saturate(wet);
    rough=clamp(rough-.12f*saturate(wet),.045f,1.f);
    return true;
}

void projectLightFinish(int kind,float3 local,float3 n,float3 v,float3 l,float3 base,
    float3 ambient,float wet,thread float3 &direct,thread float3 &ambientResponse,
    thread float &environmentScale) {
    if(kind!=15)return;
    float coverage=projectCoatCoverage(local);
    float nv=max(dot(n,v),.0001f),nl=max(dot(n,l),0.f);
    float3 halfVector=v+l;
    float nh=dot(halfVector,halfVector)>1e-12f ? max(dot(n,normalize(halfVector)),0.f):0.f;
    direct=mix(direct,shortCoatDirect(direct,base,nv,nl,nh,wet),coverage);
    ambientResponse=mix(ambientResponse,shortCoatAmbient(ambientResponse,ambient,base,nv,wet),coverage);
    environmentScale=1-coverage*.16f*(1-.80f*saturate(wet))*shortCoatE(nv);
}

// Sanctuary surface recipes. The garden path belongs to this project.
void projectSurface(int kind,float3 local,float3 world,float3 n,thread float3 &color,thread float &rough,thread float &bump,thread float &strength) {
    if(kind==15) {
        ShortCoatSample coat=shortCoatSurface(local,dfdx(local),dfdy(local),0);
        float coverage=projectCoatCoverage(local);
        color*=mix(1.f,coat.albedoMultiplier,coverage);
        bump=coat.height*coverage;strength=1;rough=coat.roughness;
        return;
    }
    if(kind==14) {
        // Regional substrate RGB is authored from continuous geography, grade and signed
        // water coverage. Modulate it rather than replacing the source's biome/bank colors.
        // Local XZ stores world metres in terrain and in the translated Soundstage patches.
        float2 p=local.xz;
        float mass=filteredNoise(p*.22),clod=filteredNoise(p*1.7);
        float aggregate=filteredNoise(p*4.5),grit=filteredNoise(p*17.);
        float pebble=smoothstep(.62,.82,aggregate);
        float patch=smoothstep(.30,.70,mass)*(.35+.65*clod);
        // A visual roughness cue from the source's dark bank albedo, not extra soil physics.
        float damp=1-smoothstep(.30,.39,max(color.r,max(color.g,color.b)));
        color*=mix(float3(.86,.91,.88),float3(1.10,1.055,.96),patch);
        color*=.91+.14*clod+.07*pebble;
        bump=(clod-.5)*.004+(grit-.5)*.0007+pebble*.002;
        strength=1;rough=mix(.96,.80,damp)+.015*(aggregate-.5);
        return;
    }
    float broad=filteredNoise(world.xz*.18),grain=filteredNoise(local.xz*18+local.y*3);
    if(kind==10) {
        // Geometry owns the sewn pattern; this source supplies quiet woven texture.
        float weave=filteredNoise(local.xz*180+local.y*27);
        float nap=filteredNoise(local.xy*11+local.z*3);
        color*=.94+.07*nap+.025*weave;
        bump=weave*.00045;strength=1;rough=.92;
    } else if(kind==13) {
        // Explicit fibres supply their own normals. Avoid amplifying subpixel
        // polygon edges with the mask's carved-grain normal perturbation.
        rough=.86;bump=0;strength=0;
    } else if(kind==11) {
        float grain=filteredNoise(local.xy*55+local.z*11);
        float age=filteredNoise(local.xz*5+local.y*3);
        color*=.80+.20*age+.035*grain;
        bump=grain*.00016+age*.0003;strength=1;rough=.78;
    } else if(kind==12) {
        float tarnish=filteredNoise(local.xy*8+local.z*4);
        color*=.70+.30*tarnish;
        bump=filteredNoise(local.xz*75+local.y)*.0007;strength=1;rough=.57;
    } else if(kind==1) {
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
