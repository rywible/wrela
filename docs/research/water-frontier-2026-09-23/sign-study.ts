import { join,resolve } from 'node:path';
import { withBrowser,fixtureServer,waitFor } from '../../../tools/browser';
import { createWaterLookdev,waterStudyCameras } from '../../../packages/model/src/water-lookdev';
const project=createWaterLookdev('ocean');
const replacements=[
 ['lateral-=shape.zw*shape.x*cos(angle)*keep*waterBody[2].y;','lateral+=shape.zw*shape.x*cos(angle)*keep*waterBody[2].y;'],
 ['jacobian+=shape.x*sin(angle)*keep*waterBody[2].y*','jacobian-=shape.x*sin(angle)*keep*waterBody[2].y*'],
 ['jac+=h*body[2].y*','jac-=h*body[2].y*'],
];
const hook=`const original=GPUDevice.prototype.createShaderModule;window.signStudyEdits=[];GPUDevice.prototype.createShaderModule=function(d){let code=d.code;for(const [a,b] of ${JSON.stringify(replacements)}){if(code.includes(a)){code=code.replaceAll(a,b);window.signStudyEdits.push(a)}}return original.call(this,{...d,code})};`;
await withBrowser(async(view,output,errors)=>{
 const server=await fixtureServer(`${hook}import {createLookdevFixture} from ${JSON.stringify(resolve('tools/fixtures/lookdev.ts'))};createLookdevFixture(${JSON.stringify({project,subject:project.entry,stage:'neutral-stage',frames:[{id:'ocean-sign-diagnostic',camera:waterStudyCameras[project.entry],time:5}],quality:'balanced'})}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,output);
 try{
  await view.navigate(String(server.url));await waitFor(view,'window.ready||window.failure',60000);
  const failure=await view.evaluate('window.failure');if(failure)throw Error(String(failure));
  const frame=await view.evaluate<any>('fixture.frame(0)');
  await Bun.write(join(output,'ocean-sign-diagnostic.png'),Buffer.from(frame.image.split(',')[1],'base64'));
  const edits=await view.evaluate('window.signStudyEdits');
  await Bun.write(join(output,'diagnostic.json'),JSON.stringify({note:'GPU-only sign diagnostic. CPU evaluator remains baseline; not a production implementation or CPU/GPU conformance result.',edits,errors},null,2));
  if(errors.length)throw Error(errors.join('\n'));
  console.log(JSON.stringify({output,edits}));
 }finally{await view.evaluate('window.fixture?.dispose()').catch(()=>{});server.stop(true)}
},1920,1080,'chrome',600000,'visual-review');
