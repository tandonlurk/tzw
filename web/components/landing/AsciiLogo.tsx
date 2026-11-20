"use client";

import { useEffect, useRef } from "react";
import logo from "./ascii-logo.json";

/** Static character hands with a separately projected spinning coin. */
export function AsciiLogo() {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    const still = document.createElement("canvas");
    const frozen = still.getContext("2d");
    if (!ctx || !frozen) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
    let frame = 0, width = 0, height = 0, last = 0, angle = .2;
    let visible = true;
    function resize() {
      if (!canvas || !ctx || !frozen) return;
      width = canvas.clientWidth;
      height = width * 2 / 3;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = still.width = Math.round(width * dpr);
      canvas.height = still.height = Math.round(height * dpr);
      ctx.setTransform(dpr,0,0,dpr,0,0);
      frozen.setTransform(dpr,0,0,dpr,0,0);
      frozen.fillStyle = "#000";
      frozen.fillRect(0,0,width,height);
      frozen.font = `${width / logo.width / .61}px monospace`;
      frozen.textBaseline = "top";
      frozen.fillStyle = "#fff";
      logo.hands.forEach((row,y)=> {
        for(let x=0;x<row.length;x++) if(row[x]!==" ") frozen.fillText(row[x],x*width/logo.width,y*height/logo.height);
      });
      draw();
    }
    function draw() {
      if(!ctx) return;
      ctx.drawImage(still,0,0,width,height);
      const cx=width*.501, cy=height*.501;
      const radius=width*.05, cosine=Math.cos(angle), sine=Math.sin(angle);
      const cell=width/logo.width, rowH=height/logo.height;
      const face=Math.abs(cosine), thickness=radius*.10*Math.abs(sine);
      const half=radius*face+thickness;
      ctx.font=`${cell/.61}px monospace`;
      ctx.textBaseline="middle";
      ctx.textAlign="center";
      for(let y=-radius;y<=radius;y+=rowH) {
        const extent=Math.sqrt(Math.max(0,1-(y/radius)**2));
        for(let x=-half;x<=half;x+=cell) {
          if(Math.abs(x)>half*extent) continue;
          const faceX=(x-Math.sign(sine)*thickness*.35)/Math.max(face,.015);
          let char="|";
          if(Math.abs(faceX)<=radius*extent && face>.08) {
            const tx=Math.max(0,Math.min(47,Math.floor((faceX/radius*(cosine<0?-1:1)+1)*24)));
            const ty=Math.max(0,Math.min(47,Math.floor((y/radius+1)*24)));
            char=logo.coin[ty][tx];
          }
          ctx.fillStyle=`rgba(255,255,255,${.45+.55*face})`;
          ctx.fillText(char,cx+x,cy+y);
        }
      }
      ctx.textAlign="start";
    }
    function animate(now:number) {
      if (visible && !reduce.matches && document.documentElement.dataset.reduceMotion !== "true") {
        if(now-last>40) { angle+=Math.min(now-last,80)*.00065; last=now; draw(); }
        frame=requestAnimationFrame(animate);
      }
    }
    function resume() {
      cancelAnimationFrame(frame);
      last=performance.now();
      if(reduce.matches) { angle=.2; draw(); }
      else if(visible) frame=requestAnimationFrame(animate);
    }
    const observer=new ResizeObserver(resize);
    observer.observe(canvas);
    const intersection=new IntersectionObserver(([entry])=>{visible=entry.isIntersecting;resume();});
    intersection.observe(canvas);
    reduce.addEventListener("change",resume);
    resize();
    return ()=>{cancelAnimationFrame(frame);observer.disconnect();intersection.disconnect();reduce.removeEventListener("change",resume);};
  }, []);
  return <canvas ref={ref} className="landing-ascii" role="img" aria-label="White ASCII art of two outstretched hands reaching toward a rotating Z coin" />;
}
