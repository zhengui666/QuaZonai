import {it,expect,vi} from 'vitest';
it('real QueryObserver pauses future polling, retains UNKNOWN and refreshes on reopen',async()=>{
 vi.stubGlobal('window',{});vi.resetModules();vi.useFakeTimers();
 const {QueryClient,QueryObserver}=await import('@tanstack/react-query');
 const client=new QueryClient({defaultOptions:{queries:{retry:false,gcTime:Infinity}}});
 let calls=0;
 const options=(enabled:boolean)=>({queryKey:['account-read'],queryFn:async()=>{calls++;return {requestId:'original',status:'UNKNOWN',read:calls};},enabled,staleTime:0,refetchInterval:3000,refetchIntervalInBackground:false});
 const observer=new QueryObserver(client,options(false));const unsubscribe=observer.subscribe(()=>{});
 try{
  await vi.advanceTimersByTimeAsync(9000);expect(calls).toBe(0);
  observer.setOptions(options(true));await vi.advanceTimersByTimeAsync(0);expect(calls).toBe(1);
  await vi.advanceTimersByTimeAsync(3000);expect(calls).toBe(2);
  observer.setOptions(options(false));await vi.advanceTimersByTimeAsync(9000);expect(calls).toBe(2);
  expect(observer.getCurrentResult().data).toEqual({requestId:'original',status:'UNKNOWN',read:2});
  observer.setOptions(options(true));await vi.advanceTimersByTimeAsync(0);expect(calls).toBe(3);
  expect(observer.getCurrentResult().data).toEqual({requestId:'original',status:'UNKNOWN',read:3});
 }finally{unsubscribe();client.clear();vi.useRealTimers();vi.unstubAllGlobals();}
});
