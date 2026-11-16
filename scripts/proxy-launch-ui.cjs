const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), { spawn } = require('node:child_process');
const {pathToFileURL} = require('node:url');
const root = path.resolve(__dirname,'..'), base = fs.mkdtempSync(path.join(os.tmpdir(),'cockpit-country-ui-'));
const output = path.join(root,'proxy-ui-evidence');
fs.mkdirSync(output,{recursive:true});
const mocks = {
 useCodexProxyExitEditor: "const noop=()=>{}; const editor={account:{id:'fixture'}, sourceId:'',itemId:'',groupId:'',selections:{},clientDefaults:false,proxyDisabled:false,bound:false,select:noop,clearError:noop}; export const useCodexProxyExitEditor=()=>editor; export const useCodexProxyAccountName=()=>()=>'示例账号';",
 CodexProxyWorkspaceContext:"const noop=()=>{}; export const useCodexProxyWorkspace=()=>({catalog:{sources:[]},unified:{},reloadCatalog:noop,acceptCatalog:noop,reloadUnified:noop,goSection:noop}); export const CodexProxyWorkspaceProvider=({children})=>children;",
 useProxyLatency:"export const useProxyLatency=()=>({});",
 CodexProxyRuntimeStatus:"export const useCodexProxyRuntimeStatus=()=>({}); export const CodexProxyRuntimeStatusPanel=()=>null;",
};
let browser, socket;
const wait=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 await require('esbuild').build({entryPoints:[path.join(__dirname,'proxy-launch-ui.fixture.tsx')],bundle:true,outfile:path.join(base,'fixture.js'),format:'iife',jsx:'automatic',loader:{'.css':'css','.png':'dataurl','.svg':'dataurl','.woff2':'dataurl'},plugins:[{name:'isolated-accounts',setup(build){build.onResolve({filter:/(useCodexProxyExitEditor|CodexProxyWorkspaceContext|useProxyLatency|CodexProxyRuntimeStatus)$/}, args=>({path:args.path.split('/').at(-1),namespace:'fixture'}));build.onLoad({filter:/.*/,namespace:'fixture'},args=>({contents:mocks[args.path],loader:'js'}));}}]});
 fs.writeFileSync(path.join(base,'index.html'),'<!doctype html><html lang="zh-CN"><meta charset="UTF-8"><link rel="stylesheet" href="fixture.css"><div id="root"></div><script src="fixture.js"></script></html>');
 const port=await new Promise((resolve,reject)=>{const s=require('node:net').createServer();s.once('error',reject);s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p));});});
 const browserPath=['C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe','C:/Program Files/Google/Chrome/Application/chrome.exe'].find(fs.existsSync);
 if(!browserPath)throw Error('No installed Windows Chromium');
 browser=spawn(browserPath,['--headless=new','--window-size=1440,1100','--disable-gpu','--no-first-run','--remote-debugging-address=127.0.0.1','--remote-debugging-port='+port,'--user-data-dir='+path.join(base,'profile'),pathToFileURL(path.join(base,'index.html')).href],{windowsHide:true,stdio:'ignore'});
 let pages;for(let n=0;n<60;n++){try{pages=await(await fetch('http://127.0.0.1:'+port+'/json')).json();if(pages.some(p=>p.type==='page'))break;}catch{}await wait(250);}
 const page=pages?.find(p=>p.type==='page');if(!page)throw Error('Browser debug page missing');
 socket=new WebSocket(page.webSocketDebuggerUrl);await new Promise(r=>socket.addEventListener('open',r,{once:true}));
 let id=0;const pending=new Map();
 socket.addEventListener('message',event=>{const reply=JSON.parse(event.data),p=pending.get(reply.id);if(p){pending.delete(reply.id);reply.error?p.reject(Error(reply.error.message)):p.resolve(reply.result);}});
 const send=(method,params={})=>new Promise((resolve,reject)=>{pending.set(++id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));});
 const ev=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(r.exceptionDetails.exception?.description||r.exceptionDetails.text);return r.result.value;};
 const assert=(v,m)=>{if(!v)throw Error(m);};
 for(let n=0;n<40;n++){if(await ev("!!document.querySelector('[role=switch]:not(:disabled)')"))break;await wait(250);}
 assert(await ev("document.querySelectorAll('[role=radio]').length===4"),'Four account modes missing');
 assert(await ev("!document.querySelector('.codex-proxy-env-editor')"),'ENV visible in follow mode');
 assert(await ev("!document.querySelector('.codex-proxy-country-evidence').open"),'Evidence not collapsed');
 const shot=async name=>{const result=await send('Page.captureScreenshot',{format:'png'});fs.writeFileSync(path.join(output,name+'.png'),Buffer.from(result.data,'base64'));};
 await shot('follow');
 await ev("document.querySelectorAll('[role=radio]')[2].click()");await wait(300);
 assert(await ev("!!document.querySelector('.codex-proxy-env-editor textarea')"),'ENV missing in client mode');
 assert(await ev("document.querySelector('.codex-proxy-env-editor textarea').value.includes('HTTPS_PROXY=')"),'ENV recognition missing');
 await shot('client');
 await ev("document.querySelector('.codex-proxy-country-evidence').open=true; document.querySelector('.codex-proxy-country-guard').scrollIntoView({block:'end'})");await wait(100);
 await shot('country');
 for(const index of [0,1,3]){await ev("document.querySelectorAll('[role=radio]')["+index+"].click()");await wait(100);assert(await ev("!document.querySelector('.codex-proxy-env-editor')"),'ENV visible in non-client mode '+index);}
 await ev("document.documentElement.dataset.theme='dark';document.querySelectorAll('[role=radio]')[2].click()");await wait(200);
 await shot('client-dark');
 fs.writeFileSync(path.join(output,'result.json'),JSON.stringify({passed:true,checks:['four modes','ENV only in client mode','recognized HTTPS variable','collapsed evidence','light and dark rendering'],syntheticAccounts:true,desktopInstalled:false},null,2));
 console.log('Proxy launch UI checks passed; screenshots saved.');
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{socket?.close();if(browser){browser.kill();await wait(1000);}try{fs.rmSync(base,{recursive:true,force:true});}catch{}});
