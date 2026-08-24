import { MAX_SAFE } from "./exact";
const pattern = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,3}))?)?$/;
export function parseInstant(value: string, timezone: "local" | string): number {
  const match = pattern.exec(value.trim()); if (!match) throw new Error("Choose a complete local date and time");
  const parts = match.slice(1).map((part) => Number(part ?? 0)); const [year,month,day,hour,minute,second=0,millis=0]=parts as [number,number,number,number,number,number,number];
  let timestamp: number;
  if (timezone === "local") timestamp = new Date(year,month-1,day,hour,minute,second,millis).getTime();
  else {
    let formatter: Intl.DateTimeFormat; try { formatter=new Intl.DateTimeFormat("en-CA",{timeZone:timezone,year:"numeric",month:"2-digit",day:"2-digit",hour:"2-digit",minute:"2-digit",second:"2-digit",hourCycle:"h23"}); } catch { throw new Error("Enter a valid IANA timezone"); }
    const naive=Date.UTC(year,month-1,day,hour,minute,second,millis), matches:number[]=[];
    for(let offset=-14*60;offset<=14*60;offset+=15){const candidate=naive-offset*60_000;const values=Object.fromEntries(formatter.formatToParts(candidate).map(item=>[item.type,item.value]));if(Number(values.year)===year&&Number(values.month)===month&&Number(values.day)===day&&Number(values.hour)===hour&&Number(values.minute)===minute&&Number(values.second)===second)matches.push(candidate+millis);}
    const unique=[...new Set(matches)]; if(unique.length===0)throw new Error("That local time does not exist in this timezone"); if(unique.length>1)throw new Error("That local time is ambiguous in this timezone"); timestamp=unique[0]!;
  }
  const microseconds=BigInt(timestamp)*1000n; if(microseconds<0n||microseconds>MAX_SAFE)throw new Error("Instant is outside the browser-safe range"); return Number(microseconds);
}
export const instantPreview=(microseconds:number)=>new Intl.DateTimeFormat(undefined,{dateStyle:"full",timeStyle:"long"}).format(new Date(microseconds/1000));
