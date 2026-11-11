import json,glob,sys
from faster_whisper import WhisperModel
m=WhisperModel("medium.en",device="cpu",compute_type="int8")
out={}
for f in sorted(glob.glob("wav/*.wav")):
    segs,_=m.transcribe(f,word_timestamps=True,vad_filter=False,condition_on_previous_text=False,
        initial_prompt="Umm, uh, so, like, Revive, Revive, flatline, flatlined, Sputnik DAO, NEAR testnet, Civic Heart, Meme Cooked.")
    S=[]
    for s in segs:
        S.append({"s":round(s.start,2),"e":round(s.end,2),"t":s.text,"w":[[round(w.start,2),round(w.end,2),w.word,round(w.probability,2)] for w in s.words]})
    out[f[-6:-4]]=S; print(f, flush=True)
json.dump(out,open("words.json","w"))
