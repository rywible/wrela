// Earth atmosphere, kilometre units internally. Rayleigh, Mie and ozone.
constant float Rg=6360.,Rt=6460.;
float2 sphereRange(float3 o,float3 d,float r) {float b=dot(o,d),len=length(o),c=(len-r)*(len+r),h=b*b-c;return h<0 ? float2(-1):float2(-b-sqrt(h),-b+sqrt(h));}
void medium(float h,thread float3 &scattering,thread float3 &extinction,thread float3 &rayleigh,thread float &mie,float haze=1.) {
    rayleigh=float3(.005802,.013558,.033100)*exp(-max(h,0.)/8.);
    mie=.003996*haze*exp(-max(h,0.)/1.2);
    float ozone=max(0.,1.-abs(h-25.)/15.);
    scattering=rayleigh+mie;extinction=rayleigh+mie*(.004440/.003996)+float3(.000650,.001881,.000085)*ozone;
}
float3 transmittance(float3 p,float3 d,float haze) {
    float2 ground=sphereRange(p,d,Rg);if(ground.x>0.001)return 0;
    float t=sphereRange(p,d,Rt).y,dt=t/32.;float3 od=0;
    for(uint i=0;i<32;i++){float3 s,e,r;float m;medium(length(p+d*((i+.5)*dt))-Rg,s,e,r,m,haze);od+=e*dt;}
    return exp(-od);
}
// Bruneton's distance-to-top parameterization allocates samples at the horizon.
// Multiple scattering has a separate, linear height / sun-zenith domain.
float2 multiUV(float height,float mu) {return float2(mu*.5+.5,clamp(height/(Rt-Rg),0.,1.));}
float2 transUV(float r,float mu) {
    float H=sqrt(Rt*Rt-Rg*Rg),rho=sqrt(max(0.,(r-Rg)*(r+Rg)));
    float d=max(0.,-r*mu+sqrt(max(0.,r*r*(mu*mu-1)+Rt*Rt)));
    float lo=Rt-r,hi=rho+H;
    float2 uv=float2((d-lo)/max(hi-lo,.001),rho/H);
    return (clamp(uv,0.,1.)*float2(255,63)+.5)/float2(256,64);
}
float3 sampleTrans(texture2d<float> lut,float3 p,float3 sun) {
    constexpr sampler smp(filter::linear,address::clamp_to_edge);
    float2 g=sphereRange(p,sun,Rg);if(g.x>.001)return 0;
    return lut.sample(smp,transUV(length(p),dot(normalize(p),sun))).rgb;
}
kernel void atmosphereTrans(texture2d<float,access::write> lut [[texture(0)]],constant Uniforms &u [[buffer(0)]],uint2 id [[thread_position_in_grid]]) {
    float2 uv=float2(id)/float2(lut.get_width()-1,lut.get_height()-1);
    float H=sqrt(Rt*Rt-Rg*Rg),rho=H*uv.y,r=sqrt(rho*rho+Rg*Rg);
    float d=mix(Rt-r,rho+H,uv.x),mu=d<.001 ? 1.:(H*H-rho*rho-d*d)/(2*r*d);
    lut.write(float4(transmittance(float3(0,r+.001,0),float3(sqrt(max(0.,1-mu*mu)),mu,0),u.sky.z),1),id);
}
// Hillaire multiple-scattering closure: L_second / (1 - f_ms).
kernel void atmosphereMultiple(texture2d<float> trans [[texture(0)]],texture2d<float,access::write> lut [[texture(1)]],constant Uniforms &u [[buffer(0)]],uint2 id [[thread_position_in_grid]]) {
    float2 uv=(float2(id)+.5)/float2(lut.get_width(),lut.get_height());float h=uv.y*100,mu=uv.x*2-1;
    float3 p=float3(0,Rg+h+.001,0),sun=float3(sqrt(max(0.,1-mu*mu)),mu,0),L=0,F=0;
    for(uint i=0;i<64;i++) {
        float y=1.-2.*(i+.5)/64.,phi=i*2.39996323;
        float3 d=float3(sqrt(1-y*y)*cos(phi),y,sqrt(1-y*y)*sin(phi));
        float t=sphereRange(p,d,Rt).y;float2 g=sphereRange(p,d,Rg);bool ground=g.x>0.;if(ground)t=min(t,g.x);
        float dt=t/20.;float3 throughput=1,localL=0,localF=0;
        for(uint j=0;j<20;j++) {
            float3 q=p+d*((j+.5)*dt),s,e,r;float m;medium(length(q)-Rg,s,e,r,m,u.sky.z);
            float3 stepT=exp(-e*dt),integral=(1-stepT)/max(e,1e-7);
            localL+=throughput*s*(sampleTrans(trans,q,sun)/(4*PI))*integral;
            localF+=throughput*s*integral;throughput*=stepT;
        }
        if(ground){float3 q=p+d*t;localL+=throughput*.25/PI*max(dot(normalize(q),sun),0.)*sampleTrans(trans,q+normalize(q)*.001,sun);}
        L+=localL/64.;F+=localF/64.;
    }
    lut.write(float4(L/max(1.-F,.02),1),id);
}
float3 skyDirection(float2 uv) {float phi=uv.x*2*PI,v=uv.y*2-1,y=sin(sign(v)*v*v*PI*.5);return float3(cos(phi)*sqrt(1-y*y),y,sin(phi)*sqrt(1-y*y));}
float2 skyUV(float3 d) {return float2(atan2(d.z,d.x)/(2*PI)+step(d.z,0.),sign(d.y)*sqrt(abs(asin(clamp(d.y,-1.,1.)))/(PI*.5))*.5+.5);}
// Cloud caches allocate every texel to the visible hemisphere. This doubles
// horizontal detail at the same memory cost as the old full-sphere cache.
float2 skyCacheUV(texture2d<float> sky,float3 d) {float2 uv=skyUV(d);if(sky.get_width()>128)uv.y=max(0.,uv.y*2-1);return uv;}
float2 reprojectSkyUV(texture2d<float> sky,float3 ray,float age) {
    if(age<=0 || ray.y<=0)return skyCacheUV(sky,ray);
    // A continuous spherical pullback cannot fold at cloud/clear boundaries.
    // Per-pixel depth refinement tore transparent silhouettes when old samples
    // selected different cloud layers. A representative shell deliberately
    // trades layer-specific parallax for stable motion; fresh caches correct it.
    float distance=sphereRange(float3(0,Rg+.002,0),ray,Rg+2.2).y;
    float3 flow=float3(.0134,0,.004);
    float fade=smoothstep(0.,.015,ray.y);
    return skyCacheUV(sky,normalize(ray*distance-flow*(age*fade)));
}
float cloudHash(int3 cell) {
    uint3 c=uint3(cell)&15u;
    uint h=c.x*1597334677u ^ c.y*3812015801u ^ c.z*958282917u;h=(h^(h>>16))*2246822519u;h=(h^(h>>13))*3266489917u;h^=h>>16;
    return float(h&0xffffffu)/16777215.;
}
kernel void atmosphereNoise(texture3d<float,access::write> output [[texture(0)]],uint3 id [[thread_position_in_grid]]) {
    float3 p=(float3(id)+.5)/8.;int3 cell=int3(floor(p));float nearest=2.;
    for(int z=-1;z<=1;z++)for(int y=-1;y<=1;y++)for(int x=-1;x<=1;x++) {
        int3 c=cell+int3(x,y,z);
        float3 feature=float3(c)+.1+.8*float3(cloudHash(c),cloudHash(c+int3(7,3,11)),cloudHash(c+int3(2,13,5)));
        nearest=min(nearest,length(p-feature));
    }
    float3 f=fract(p);f=f*f*(3.-2.*f);
    float value=0;
    for(int z=0;z<2;z++)for(int y=0;y<2;y++)for(int x=0;x<2;x++) {
        float3 w=mix(1.-f,f,float3(x,y,z));value+=cloudHash(cell+int3(x,y,z))*w.x*w.y*w.z;
    }
    // Both channels have a 16 km noise-space period, with C1 value noise.
    output.write(float4(value,1-clamp(nearest,0.,1.),0,1),id);
}
float cloudNoise(texture3d<float> noise,float3 p) {constexpr sampler s(filter::linear,address::repeat);return noise.sample(s,p/16.).r;}
float cloudBillow(texture3d<float> noise,float3 p) {constexpr sampler s(filter::linear,address::repeat);return noise.sample(s,p/16.).g;}
// A continuous moisture field supplies shared clear regions, clustered convection
// and connected banks. No independently placed ellipsoid/cloud template exists.
float4 cloudMacro(texture3d<float> noise,float3 p,constant Uniforms &u) {
    float h=p.y;if(u.sky.x<=0)return float4(-1,0,0,0);
    if(h<1.5 || h>4.8)return float4(-1,0,0,0);
    float time=u.cameraTime.w*u.options.y;
    p.xz-=float2(.012+(h-1.5)*.002,.004)*time;
    float3 offset=float3(u.sky.w*1.717,2.1,u.sky.w*.931);
    float2 rotated=float2(.8*p.x-.6*p.z,.6*p.x+.8*p.z);
    float moisture=.65*cloudNoise(noise,float3(rotated.x*.045,1,rotated.y*.045)+offset)
                  +.35*cloudNoise(noise,float3(p.x*.13,7,p.z*.13)+offset);
    float localCoverage=clamp(u.sky.x+(moisture-.5)*1.6,0.,1.);
    float convection=cloudNoise(noise,float3(p.x*.19,4,p.z*.19)+offset);
    float top=2.15+2.5*smoothstep(.25,.8,convection);
    float height=(h-1.5)/(top-1.5);
    // Density fades near the condensation level and the locally varying top.
    float profile=smoothstep(0.,.065,height)*(1-smoothstep(.35,1.,height));
    float3 q=p+offset; q.y-=time*.0006;
    float shape=.52*cloudNoise(noise,q*.55)+.10*cloudNoise(noise,q*1.13)
               +.24*cloudBillow(noise,q*1.45)+.14*cloudBillow(noise,q*3.7);
    float field=shape-(.87-localCoverage*.6)-(1-profile)*.32;
    float deckAmount=smoothstep(.73,.94,u.sky.x);
    float deck=1.5*smoothstep(1.5,1.7,h)*(1-smoothstep(2.2,2.6,h))
               *(.7+.6*cloudNoise(noise,q*.75));
    // Cache the signed shape before clamping density. Interpolating already
    // clamped density smears every boundary over the coarse voxel footprint.
    return float4(field,smoothstep(1.5,1.56,h)*(1-deckAmount)*u.sky.y,deck*deckAmount*u.sky.y,0);
}
constant float cloudExtent=32.;
float3 cloudVolumeUV(float3 p) {return float3(p.x/(2*cloudExtent)+.5,(p.y-1.5)/3.3,p.z/(2*cloudExtent)+.5);}
float cachedCloud(texture3d<float> density,texture3d<float> noise,float3 p,constant Uniforms &u,texture2d<float> weather) {
    if(p.y<1.5 || p.y>4.8)return 0;
    if(max(abs(p.x),abs(p.z))>=cloudExtent)
        return 1.5*smoothstep(.73,.94,u.sky.x)*smoothstep(1.5,1.7,p.y)*(1-smoothstep(2.2,2.6,p.y))*u.sky.y;
    constexpr sampler s(filter::linear,address::clamp_to_edge);
    float finiteCoverage=mix(1-smoothstep(24.,cloudExtent,max(abs(p.x),abs(p.z))),1.,smoothstep(.73,.94,u.sky.x));
    float time=u.cameraTime.w*u.options.y;
    p.xz-=float2(.012+(p.y-1.5)*.002,.004)*time;
    constexpr sampler regional(filter::linear,address::repeat);
    float4 air=weather.sample(regional,p.xz/(2*cloudExtent)+.5);
    // Buoyant moist columns raise cloud crowns; condensation/evaporation changes
    // optical density. Both primary and shadow rays use this same reconstruction.
    p.y=1.5+(p.y-1.5)/clamp(1+air.z*.035,.80,1.28);
    p.xz=(fract(p.xz/(2*cloudExtent)+.5)-.5)*2*cloudExtent;
    float potential=density.sample(s,cloudVolumeUV(p)).r;
    float profile=smoothstep(1.5,1.7,p.y)*(1-smoothstep(3.8,4.8,p.y));
    potential=potential*clamp(.7+air.x*1.5,.6,1.8)+(air.x-.2)*.16*profile*u.sky.y;
    if(potential>0 && potential<.4 && u.sky.x<.8) {
        // Erode existing density only. Adding sub-step features outside the
        // boundary produced isolated sparkling specks instead of resolved wisps.
        float3 q=p+float3(u.sky.w*1.717,2.1,u.sky.w*.931);
        potential-=(1-cloudBillow(noise,q*33.1))*.065*(1-smoothstep(.18,.4,potential))*profile*u.sky.y;
    }
    return max(0.,potential)*finiteCoverage;
}
float cloudOpticalDepth(texture3d<float> lighting,float3 p,constant Uniforms &u) {
    if(p.y<1.5 || p.y>4.8)return 0;
    if(max(abs(p.x),abs(p.z))>=cloudExtent)
        return max(0.,2.6-p.y)*12.*u.sky.y*smoothstep(.73,.94,u.sky.x)/max(.05,u.sunExposure.y);
    constexpr sampler s(filter::linear,address::clamp_to_edge);
    return lighting.sample(s,cloudVolumeUV(p)).r;
}
float cloudShadow(texture3d<float> lighting,float3 p,constant Uniforms &u) {
    return exp(-cloudOpticalDepth(lighting,p,u));
}
kernel void atmosphereCloudDensity(texture3d<float> noise [[texture(0)]],texture3d<float,access::write> density [[texture(1)]],constant Uniforms &u [[buffer(0)]],uint3 id [[thread_position_in_grid]]) {
    id.y+=uint(u.environment.w);
    float3 uv=(float3(id)+.5)/float3(density.get_width(),density.get_height(),density.get_depth());
    float3 p=float3((uv.x-.5)*2*cloudExtent,1.5+uv.y*3.3,(uv.z-.5)*2*cloudExtent);
    float edge=1-smoothstep(24.,cloudExtent,max(abs(p.x),abs(p.z)));
    float4 shape=cloudMacro(noise,p,u);shape.y*=edge;
    float time=u.cameraTime.w*u.options.y;
    p.xz-=float2(.012+(p.y-1.5)*.002,.004)*time;
    float3 q=p+float3(u.sky.w*1.717,2.1,u.sky.w*.931);q.y-=time*.0006;
    float detail=(cloudBillow(noise,q*5.7)-.45)*.07+(cloudBillow(noise,q*17.3)-.45)*.018;
    float value=(shape.x+detail)*12.*shape.y;
    if(u.sky.x>.73)value=max(value,0.)+shape.z;
    // Signed, high-resolution density preserves boundaries under interpolation.
    density.write(float4(value,0,0,0),id);
}
float propagatedOptical(texture3d<float,access::read_write> lighting,float2 pixel,uint layer) {
    int2 cell=int2(floor(pixel));float2 f=fract(pixel);float optical=0;
    for(int z=0;z<2;z++)for(int x=0;x<2;x++) {
        int2 c=cell+int2(x,z);
        if(any(c<0) || c.x>=int(lighting.get_width()) || c.y>=int(lighting.get_depth()))continue;
        float weight=(x ? f.x:1-f.x)*(z ? f.y:1-f.y);
        optical+=lighting.read(uint3(c.x,layer,c.y)).r*weight;
    }
    return optical;
}
kernel void atmosphereCloudLighting(texture3d<float> density [[texture(0)]],texture3d<float,access::read_write> lighting [[texture(1)]],texture3d<float> noise [[texture(2)]],texture2d<float> weather [[texture(3)]],constant Uniforms &u [[buffer(0)]],uint3 id [[thread_position_in_grid]]) {
    // Each dispatch builds one altitude slice, in sunlight order. Propagate the
    // optical depth already integrated above instead of tracing 32 new rays at
    // every voxel. Separate encoder dispatches establish inter-slice ordering.
    uint slice=uint(u.environment.w);bool downward=u.sunExposure.y>=0;
    id.y=downward ? lighting.get_height()-1-slice:slice;
    float3 uv=(float3(id)+.5)/float3(lighting.get_width(),lighting.get_height(),lighting.get_depth());
    float3 p=float3((uv.x-.5)*2*cloudExtent,1.5+uv.y*3.3,(uv.z-.5)*2*cloudExtent);
    float dt=(3.3/lighting.get_height())/max(abs(u.sunExposure.y),.01);
    float3 upstream=p+u.sunExposure.xyz*dt;
    float2 pixel=float2(upstream.x,upstream.z)/(2*cloudExtent)*float2(lighting.get_width(),lighting.get_depth())+.5*float2(lighting.get_width()-1,lighting.get_depth()-1);
    float optical=slice==0 ? 0:propagatedOptical(lighting,pixel,downward ? id.y+1:id.y-1);
    uint count=uint(clamp(ceil(dt/.15),1.,12.));
    for(uint i=0;i<count;i++)optical+=cachedCloud(density,noise,p+u.sunExposure.xyz*((i+.5)*dt/count),u,weather)*(dt/count)*8.;
    lighting.write(float4(min(optical,80.),0,0,0),id);
}
// Max reduction is conservative: no averaged mip can certify empty space.
kernel void atmosphereMaxDensity(texture3d<half,access::read> source [[texture(0)]],texture3d<half,access::write> target [[texture(1)]],uint3 id [[thread_position_in_grid]]) {
    half value=-INFINITY;
    for(uint z=0;z<2;z++)for(uint y=0;y<2;y++)for(uint x=0;x<2;x++)value=max(value,source.read(id*2+uint3(x,y,z)).r);
    target.write(half4(value),id);
}
kernel void atmosphereEmptyCells(texture3d<half,access::read> source [[texture(0)]],texture3d<half,access::write> target [[texture(1)]],uint3 id [[thread_position_in_grid]]) {
    half value=-INFINITY;
    // A neighboring-cell halo encloses the trilinear reconstruction footprint.
    for(int z=-1;z<=1;z++)for(int x=-1;x<=1;x++)for(int y=-1;y<=1;y++) {
        uint3 c=uint3((int(id.x)+x+128)%128,clamp(int(id.y)+y,0,11),(int(id.z)+z+128)%128);
        value=max(value,source.read(c).r);
    }
    target.write(half4(value),id);
}
float emptyCloudStep(texture3d<float,access::read> cells,texture2d<float> weather,float3 local,float3 ray,float distance,constant Uniforms &u) {
    if(u.sky.x>.73 || max(abs(local.x),abs(local.z))>=cloudExtent)return 0;
    float time=u.cameraTime.w*u.options.y;
    float3 p=local;p.xz-=float2(.012+(local.y-1.5)*.002,.004)*time;
    constexpr sampler regional(filter::linear,address::repeat);
    float stretch=clamp(1+weather.sample(regional,p.xz/(2*cloudExtent)+.5).z*.035,.80,1.28);
    p.y=1.5+(p.y-1.5)/stretch;
    if(p.y<=1.5 || p.y>=4.8)return 0;
    float3 cell=float3(fract(p.x/(2*cloudExtent)+.5)*128.,(p.y-1.5)/3.3*12.,fract(p.z/(2*cloudExtent)+.5)*128.);
    float maximum=cells.read(uint3(cell)).r;
    float upper=maximum*(maximum>0 ? 1.8:.6)+max(0.,u.weatherBounds.x-.2)*.16*u.sky.y;
    if(upper>-.0001)return 0;
    float3 gap=min(fract(cell),1-fract(cell))*float3(.5,3.3/12.,.5);
    // Jacobian bound for the spherical ray, shear, and bilinear moist updraft
    // deformation over <= 0.5 km. The CPU supplies maximum adjacent-cell gradients.
    float hPrime=(distance+(Rg+.002)*ray.y)/(Rg+local.y);
    float vx=abs(ray.x-.002*time*hPrime)+abs(time)*.002*.5/Rg,vz=abs(ray.z);
    float vy=min(1.,abs(hPrime)+.5/Rg)/.8+3.8*(u.weatherBounds.y*vx+u.weatherBounds.z*vz)/.64;
    float3 steps=gap/max(float3(vx,vy,vz),1e-6);
    return min(.5,min(steps.x,min(steps.y,steps.z)))*.99;
}
kernel void atmosphereVerifyEmptySpace(texture3d<float> density [[texture(0)]],texture3d<float> noise [[texture(1)]],texture2d<float> weather [[texture(2)]],texture3d<float,access::read> cells [[texture(3)]],constant Uniforms &u [[buffer(0)]],device atomic_uint *result [[buffer(1)]],uint id [[thread_position_in_grid]]) {
    uint bits=id*747796405u+2891336453u;bits=((bits>>((bits>>28u)+4u))^bits)*277803737u;bits=(bits>>22u)^bits;
    float y=.005+.995*float(bits&65535u)/65535.,phi=float(bits>>16)*2*PI/65535.;
    float3 d=float3(cos(phi)*sqrt(1-y*y),y,sin(phi)*sqrt(1-y*y)),origin=float3(0,Rg+.002,0);
    float start=sphereRange(origin,d,Rg+1.5).y,end=sphereRange(origin,d,Rg+4.8).y;
    float t=mix(start,end,fract(float(id)*.61803398875));
    float3 p=origin+d*t,local=float3(p.x,length(p)-Rg,p.z);
    float step=emptyCloudStep(cells,weather,local,d,t,u);
    if(step<=.03)return;
    atomic_fetch_add_explicit(result,1,memory_order_relaxed);
    for(uint i=0;i<9;i++) {
        float3 q=origin+d*(t+step*float(i)/8.);
        float value=cachedCloud(density,noise,float3(q.x,length(q)-Rg,q.z),u,weather);
        atomic_fetch_max_explicit(result+2,as_type<uint>(max(value,0.)),memory_order_relaxed);
        if(value>1e-5)atomic_fetch_add_explicit(result+1,1,memory_order_relaxed);
    }
}
float phaseHG(float c,float g){return (1-g*g)/(4*PI*pow(max(.001,1+g*g-2*g*c),1.5));}
struct AirIntegral {float3 radiance;float3 foreground;float3 transmission;};
AirIntegral integrateAir(float3 d,texture2d<float> trans,texture2d<float> multiple,texture3d<float> cloudLighting,constant Uniforms &u) {
    constexpr sampler smp(filter::linear,address::clamp_to_edge);
    float3 p=float3(0,Rg+.002,0),sun=u.sunExposure.xyz;
    float t=sphereRange(p,d,Rt).y;float2 g=sphereRange(p,d,Rg);if(g.x>0)t=min(t,g.x);
    float cosTheta=dot(d,sun),phaseR=3./(16*PI)*(1+cosTheta*cosTheta),anisotropy=.8;
    float phaseM=3./(8*PI)*(1-anisotropy*anisotropy)*(1+cosTheta*cosTheta)/((2+anisotropy*anisotropy)*pow(1+anisotropy*anisotropy-2*anisotropy*cosTheta,1.5));
    float start=max(0.,sphereRange(p,d,Rg+1.5).y);
    float3 L=0,T=1,frontL=0,frontT=1;
    for(uint i=0;i<40;i++) {
        float a=float(i)/40,b=float(i+1)/40,dt=(b*b-a*a)*t;
        float3 q=p+d*((a*a+b*b)*.5*t),s,e,r;float m;medium(length(q)-Rg,s,e,r,m,u.sky.z);
        float visibility=1.,height=length(q)-Rg;
        if(height<4.8 && sun.y>.015 && u.sky.x>0 && u.environment.x!=4) {
            float3 local=float3(q.x,height,q.z);
            local+=sun*max(0.,(1.501-height)/sun.y);
            visibility=cloudShadow(cloudLighting,local,u);
        }
        // Cloud occlusion of low-altitude direct scattering produces shafts in
        // haze. Higher-order scattering remains the unoccluded LUT approximation.
        float3 source=sampleTrans(trans,q,sun)*(r*phaseR+m*phaseM)*visibility+multiple.sample(smp,multiUV(height,dot(normalize(q),sun))).rgb*s;
        float frontDT=clamp(start-a*a*t,0.,dt);
        frontL+=T*source*(1-exp(-e*frontDT))/max(e,1e-7);frontT*=exp(-e*frontDT);
        float3 stepT=exp(-e*dt);L+=T*source*(1-stepT)/max(e,1e-7);T*=stepT;
    }
    L*=3.5;frontL*=3.5;
    return {L,frontL,frontT};
}
// Smooth molecular scattering has a much lower bandwidth than cloud edges.
// Keep foreground radiance and transmission separate for correct cloud compositing.
kernel void atmosphereAir(texture2d<float> trans [[texture(0)]],texture2d<float> multiple [[texture(1)]],texture2d<float,access::write> radiance [[texture(2)]],texture2d<float,access::write> foreground [[texture(3)]],texture2d<float,access::write> transmission [[texture(4)]],texture3d<float> cloudLighting [[texture(5)]],constant Uniforms &u [[buffer(0)]],uint2 id [[thread_position_in_grid]]) {
    id.y+=uint(u.environment.w);
    float3 d=skyDirection((float2(id)+.5)/float2(radiance.get_width(),radiance.get_height()));
    AirIntegral a=integrateAir(d,trans,multiple,cloudLighting,u);
    radiance.write(float4(a.radiance,1),id);foreground.write(float4(a.foreground,1),id);transmission.write(float4(a.transmission,1),id);
}
kernel void atmosphereSky(texture2d<float> trans [[texture(0)]],texture2d<float> multiple [[texture(1)]],texture2d<float,access::write> sky [[texture(2)]],texture3d<float> noise [[texture(3)]],texture2d<float> cloudAmbient [[texture(4)]],texture3d<float> cloudDensity [[texture(5)]],texture3d<float> cloudLighting [[texture(6)]],texture2d<float> airRadiance [[texture(7)]],texture2d<float> airForeground [[texture(8)]],texture2d<float> airTransmission [[texture(9)]],texture2d<float> weather [[texture(10)]],texture3d<float,access::read> emptyCells [[texture(11)]],constant Uniforms &u [[buffer(0)]],uint2 id [[thread_position_in_grid]]) {
    id.y+=uint(u.environment.w);
    constexpr sampler smp(filter::linear,address::clamp_to_edge);
    float2 directionUV=(float2(id)+.5)/float2(sky.get_width(),sky.get_height());
    if(sky.get_width()>128)directionUV.y=directionUV.y*.5+.5;
    float3 d=skyDirection(directionUV),p=float3(0,Rg+.002,0),sun=u.sunExposure.xyz;
    float start=max(0.,sphereRange(p,d,Rg+1.5).y);
    float2 airUV=skyUV(d);
    AirIntegral air;
    if(sky.get_width()<=128)air=integrateAir(d,trans,multiple,cloudLighting,u);
    else {air={airRadiance.sample(smp,airUV).rgb,airForeground.sample(smp,airUV).rgb,airTransmission.sample(smp,airUV).rgb};}
    float3 L=air.radiance,frontL=air.foreground,frontT=air.transmission;
    // Thin high ice clouds remain lit after lower clouds enter Earth's shadow.
    if(d.y>.005 && u.sky.x>0 && u.environment.x!=4) {
        float distance=sphereRange(p,d,Rg+8.5).y;
        float3 q=p+d*distance,local=float3(q.x,8.5,q.z);
        local.xz-=float2(.028,.009)*u.cameraTime.w*u.options.y;
        float3 offset=float3(u.sky.w*13.,3,u.sky.w*7.);
        float2 w=float2(cloudNoise(noise,local*.16+offset),cloudNoise(noise,local*.21+offset+7.))*3.;
        float3 flow=float3(local.x*.18+w.x,9,local.z*1.1+w.y)+offset;
        float fibers=.55*cloudNoise(noise,flow)+.3*cloudNoise(noise,flow*2.07)+.15*cloudNoise(noise,flow*4.11);
        float coverage=cloudNoise(noise,float3(local.x*.09,4,local.z*.09)+offset);
        float tau=smoothstep(.42,.68,coverage)*smoothstep(.51,.72,fibers)*.006*(1-smoothstep(.7,1.,u.sky.x))*smoothstep(.05,.25,d.y);
        float alpha=1-exp(-tau/max(.25,d.y));
        float3 sunlight=sampleTrans(trans,q,sun)*3.5;
        float3 ice= sunlight*(.20+.16*phaseHG(dot(d,sun),.6))+L*.35;
        L=mix(L,ice,alpha);
    }
    float3 background=L,cloudL=0;float cloudT=1.;
    if(d.y>-.015 && u.environment.x!=4 && u.sky.x>0) {
        float end=min(u.sky.x>.73 ? 160.:cloudExtent/max(abs(d.x),abs(d.z)),sphereRange(p,d,Rg+4.8).y);
        if(end>start) {
            float c=dot(d,sun);
            float phase=.7*phaseHG(c,.65)+.3*phaseHG(c,-.25);
            // The LUT stores radiance. Isotropic incident light integrates to
            // that radiance after the phase's 1/(4π), not 4π times the value.
            float3 diffuse=multiple.sample(smp,multiUV(2.5,sun.y)).rgb*3.5;
            float3 ambient=cloudAmbient.sample(smp,skyUV(float3(0,1,0))).rgb*.7+diffuse*.2;
            float day=clamp((sun.y+.1)*2.,0.,1.);
            ambient=mix(ambient,float3(.12,.13,.145)*day,smoothstep(.72,.95,u.sky.x));
            // Stable angular dither breaks aligned integration bands; it never
            // changes when the camera moves or when a capture is repeated.
            float jitter=fract(sin(dot(float2(id),float2(12.9898,78.233)))*43758.5453);
            float distance=start+jitter*.018;float3 lastSource=0;
            for(uint i=0;i<1024 && distance<end && cloudT>.005;i++) {
                float dt=min(end-distance,.030+distance*.0015);
                float3 q=p+d*distance,local=float3(q.x,length(q)-Rg,q.z);
                float emptyStep=emptyCloudStep(emptyCells,weather,local,d,distance,u);
                if(emptyStep>dt) {distance+=min(emptyStep,end-distance);continue;}
                float density=cachedCloud(cloudDensity,noise,local,u,weather);
                if(density>.001) {
                    float localOptical=0;
                    localOptical=(density+cachedCloud(cloudDensity,noise,local+sun*.09,u,weather))*(.06*8.);
                    float tau=cloudOpticalDepth(cloudLighting,local+sun*.12,u)+localOptical;
                    float quarterT=exp(-.25*tau),softT=exp(-.03*tau),halfT=quarterT*quarterT;
                    float shadow=halfT*halfT,stepT=exp(-density*dt*8.);
                    float radius=Rg+local.y,mu=dot(q,sun)/radius;
                    float3 sunColor=mu < -sqrt(max(0.,(radius-Rg)*(radius+Rg)))/radius ? float3(0):trans.sample(smp,transUV(radius,mu)).rgb*3.5;
                    // Approximate cloud multiple scattering fills the body while
                    // directional single scattering supplies rims and silver lining.
                    float3 source=ambient*(.65+.35*softT)*(.75+.25*exp(-density))+sunColor*(phase*shadow+.075*quarterT+.02*(softT*softT));
                    lastSource=source;
                    cloudL+=cloudT*source*(1-stepT);cloudT*=stepT;
                }
                distance+=dt;
            }
            if(cloudT<.005){cloudL+=cloudT*lastSource;cloudT=0;}
            cloudL=cloudL*frontT+frontL*(1-cloudT);
        }
    }
    L=cloudL+cloudT*background;
    sky.write(float4(L,cloudT),id);
}
float3 skyLight(texture2d<float> sky,float3 d) {constexpr sampler s(filter::linear,s_address::repeat,t_address::clamp_to_edge);if(d.y<0 && sky.get_width()>128)return sky.sample(s,skyCacheUV(sky,float3(0,1,0))).rgb*float3(.18,.20,.12);return sky.sample(s,skyCacheUV(sky,d)).rgb;}
