"""Original 5x7 grid lettering for the game's DOS interface. Build-time only: fonttools."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
rows = {
'A':'0e 11 11 1f 11 11 11','B':'1e 11 11 1e 11 11 1e','C':'0e 11 10 10 10 11 0e','D':'1e 11 11 11 11 11 1e','E':'1f 10 10 1e 10 10 1f','F':'1f 10 10 1e 10 10 10','G':'0e 11 10 17 11 11 0f','H':'11 11 11 1f 11 11 11','I':'0e 04 04 04 04 04 0e','J':'07 02 02 02 12 12 0c','K':'11 12 14 18 14 12 11','L':'10 10 10 10 10 10 1f','M':'11 1b 15 15 11 11 11','N':'11 19 15 13 11 11 11','O':'0e 11 11 11 11 11 0e','P':'1e 11 11 1e 10 10 10','Q':'0e 11 11 11 15 12 0d','R':'1e 11 11 1e 14 12 11','S':'0f 10 10 0e 01 01 1e','T':'1f 04 04 04 04 04 04','U':'11 11 11 11 11 11 0e','V':'11 11 11 11 11 0a 04','W':'11 11 11 15 15 15 0a','X':'11 11 0a 04 0a 11 11','Y':'11 11 0a 04 04 04 04','Z':'1f 01 02 04 08 10 1f',
'a':'00 00 0e 01 0f 11 0f','b':'10 10 1e 11 11 11 1e','c':'00 00 0f 10 10 10 0f','d':'01 01 0f 11 11 11 0f','e':'00 00 0e 11 1f 10 0f','f':'06 09 08 1e 08 08 08','g':'00 00 0f 11 0f 01 0e','h':'10 10 1e 11 11 11 11','i':'04 00 0c 04 04 04 0e','j':'02 00 06 02 02 12 0c','k':'10 10 12 14 18 14 12','l':'0c 04 04 04 04 04 0e','m':'00 00 1a 15 15 15 15','n':'00 00 1e 11 11 11 11','o':'00 00 0e 11 11 11 0e','p':'00 00 1e 11 1e 10 10','q':'00 00 0f 11 0f 01 01','r':'00 00 16 19 10 10 10','s':'00 00 0f 10 0e 01 1e','t':'08 08 1e 08 08 09 06','u':'00 00 11 11 11 11 0f','v':'00 00 11 11 11 0a 04','w':'00 00 11 11 15 15 0a','x':'00 00 11 0a 04 0a 11','y':'00 00 11 11 0f 01 0e','z':'00 00 1f 02 04 08 1f',
'0':'0e 11 13 15 19 11 0e','1':'04 0c 04 04 04 04 0e','2':'0e 11 01 02 04 08 1f','3':'1e 01 01 0e 01 01 1e','4':'02 06 0a 12 1f 02 02','5':'1f 10 10 1e 01 01 1e','6':'0e 10 10 1e 11 11 0e','7':'1f 01 02 04 08 08 08','8':'0e 11 11 0e 11 11 0e','9':'0e 11 11 0f 01 01 0e',
' ':'00 00 00 00 00 00 00','.':'00 00 00 00 00 0c 0c',',':'00 00 00 00 04 04 08',':':'00 0c 0c 00 0c 0c 00',';':'00 0c 0c 00 04 04 08','!':'04 04 04 04 04 00 04','?':'0e 11 01 02 04 00 04','-':'00 00 00 1f 00 00 00','_':'00 00 00 00 00 00 1f','+':'00 04 04 1f 04 04 00','=':'00 00 1f 00 1f 00 00','/':'01 02 02 04 08 08 10','\\':'10 08 08 04 02 02 01','[':'0e 08 08 08 08 08 0e',']':'0e 02 02 02 02 02 0e','(':'02 04 08 08 08 04 02',')':'08 04 02 02 02 04 08','<':'01 02 04 08 04 02 01','>':'10 08 04 02 04 08 10',"'":'04 04 08 00 00 00 00','"':'0a 0a 14 00 00 00 00','*':'00 15 0e 1f 0e 15 00','%':'19 19 02 04 08 13 13','#':'0a 0a 1f 0a 1f 0a 0a','&':'0c 12 14 08 15 12 0d','@':'0e 11 17 15 17 10 0e','|':'04 04 04 04 04 04 04','~':'00 00 09 16 00 00 00','^':'04 0a 11 00 00 00 00','`':'08 04 00 00 00 00 00','$':'04 0f 14 0e 05 1e 04','{':'02 04 04 08 04 04 02','}':'08 04 04 02 04 04 08',
'↑':'04 0e 15 04 04 04 04','↓':'04 04 04 04 15 0e 04','→':'00 04 02 1f 02 04 00','←':'00 04 08 1f 08 04 00','·':'00 00 00 04 00 00 00','×':'00 11 0a 04 0a 11 00','◇':'04 0a 11 11 11 0a 04','⌄':'00 00 11 0a 04 00 00','≡':'00 1f 00 1f 00 1f 00','°':'0c 12 12 0c 00 00 00'}
for a,b in [('’',"'"),('‘',"'"),('“','"'),('”','"'),('–','-'),('—','-'),('−','-')]:rows[a]=rows[b]
font=FontBuilder(1024,isTTF=True);order=['.notdef']+[f'u{ord(c):04X}' for c in rows];font.setupGlyphOrder(order)
glyphs={};metrics={}
for c,name in [('', '.notdef')]+[(c,f'u{ord(c):04X}') for c in rows]:
    pen=TTGlyphPen(None)
    pattern=rows.get(c,'1f 11 15 15 15 11 1f')
    for y,row in enumerate(pattern.split()):
        for x in range(5):
            if int(row,16)&(1<<(4-x)):
                xx=x*128;yy=(6-y)*128
                pen.moveTo((xx,yy));pen.lineTo((xx,yy+128));pen.lineTo((xx+128,yy+128));pen.lineTo((xx+128,yy));pen.closePath()
    glyphs[name]=pen.glyph();metrics[name]=(768,0)
font.setupCharacterMap({ord(c):f'u{ord(c):04X}' for c in rows});font.setupGlyf(glyphs);font.setupHorizontalMetrics(metrics);font.setupHorizontalHeader(ascent=896,descent=-128);font.setupOS2(sTypoAscender=896,sTypoDescender=-128,usWinAscent=896,usWinDescent=128);font.setupNameTable({'familyName':'Marches Pixel','styleName':'Regular','uniqueFontIdentifier':'FantasyLand Marches Pixel 1','fullName':'Marches Pixel','psName':'MarchesPixel'});font.setupPost();font.setupMaxp()
path=Path(__file__).resolve().parent.parent/'dist/fonts/marches-pixel.ttf';path.parent.mkdir(exist_ok=True);font.save(str(path));print(path)
