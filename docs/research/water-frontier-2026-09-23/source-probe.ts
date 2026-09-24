import { createWaterLookdev } from '../../../packages/model/src/water-lookdev';
import { compileWaterSpectrum, sampleWaterSpectrum } from '../../../packages/compiler/src/water-spectrum';
const project=createWaterLookdev('ocean');
const water=project.documents.find(d=>d.id===project.entry);
if(water?.kind!=='water'||!water.spectrum) throw Error('Missing ocean');
const variants=[3,6,8,15,30].map(windSpeed=>{
 const s=compileWaterSpectrum({...water,spectrum:{...water.spectrum!,windSpeed}});
 return {windSpeed,amplitudeBound:s.amplitudeBound,slopeVariance:s.slopeVariance,choppiness:s.choppiness,carriers:Array.from(s.carriers)};
});
const spectrum=compileWaterSpectrum(water);
const determinants:number[]=[];let crestCandidates=0;let reversedCrestCandidates=0;let count=0;let heightSum=0,detSum=0,crossSum=0,heightSquared=0,detSquared=0;
for(const time of [0,5,10]) for(let iz=0;iz<80;iz++)for(let ix=0;ix<80;ix++){
 const wave=sampleWaterSpectrum(spectrum,ix*1.37,iz*1.43,time);
 const det=wave.jxx*wave.jzz-wave.jxz*wave.jxz;
 determinants.push(det);if(det<0.9&&wave.height>0)crestCandidates++;
 const corrected=(2-wave.jxx)*(2-wave.jzz)-wave.jxz*wave.jxz;
 if(corrected<0.9&&wave.height>0)reversedCrestCandidates++;
 heightSum+=wave.height;detSum+=det;crossSum+=wave.height*det;heightSquared+=wave.height**2;detSquared+=det**2;count++;
}
const smooth=(a:number,b:number,x:number)=>{const t=Math.max(0,Math.min(1,(x-a)/(b-a)));return t*t*(3-2*t)};
const budgets=[{period:112,kind:'largest'},{period:14,kind:'middle'},{period:1.75,kind:'smallest'}].map(b=>({...b,texelMetres:b.period/256}));
const result={date:'2026-09-23',wind:variants.map(({carriers,...v})=>({...v,identicalToWind6:JSON.stringify(carriers)===JSON.stringify(variants[1].carriers)})),analyticCrestProbe:{samples:count,minDeterminant:Math.min(...determinants),maxDeterminant:Math.max(...determinants),candidateFraction:crestCandidates/count,reversedChoppinessCandidateFraction:reversedCrestCandidates/count,heightDeterminantCorrelation:(count*crossSum-heightSum*detSum)/Math.sqrt((count*heightSquared-heightSum**2)*(count*detSquared-detSum**2)),note:'Analytic sample; candidate is determinant < 0.9 and positive height. Sign reversal is a proposed diagnostic. Not pixel coverage or filtered-GPU foam measurement.'},cascadeBudgets:budgets,carrierEvaluationsPerSynthesis:3*256*256*18,creekCellMetres:[28/127,48/127],gridBytes:{currentPacked:128*128*48,currentOnly:128*128*16,staticBed:128*128*16},note:'Read-only source probe. No renderer changes.'};
await Bun.write('docs/research/water-frontier-2026-09-23/source-probe.json',JSON.stringify(result,null,2));
console.log(JSON.stringify(result,null,2));
