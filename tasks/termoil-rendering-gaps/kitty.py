import base64, sys, math
w=h=96
px=bytearray()
for y in range(h):
    for x in range(w):
        d=math.hypot(x-w/2,y-h/2)
        px+=bytes([int(255*x/w), int(255*(1-d/68)) if d<68 else 0, int(255*y/h)])
data=base64.b64encode(bytes(px)).decode()
chunks=[data[i:i+4096] for i in range(0,len(data),4096)]
out=sys.stdout
out.write("kitty graphics:\n")
for i,c in enumerate(chunks):
    m=1 if i<len(chunks)-1 else 0
    head=f"a=T,f=24,s={w},v={h},m={m}" if i==0 else f"m={m}"
    out.write(f"\x1b_G{head};{c}\x1b\\")
out.write("\n"); out.flush()
