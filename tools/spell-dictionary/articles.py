# Fetches the plain text of Wikipedia articles a person might write about and writes their words, one token a line,
# to article_words.txt (with duplicates, so a count is a count of occurrences).
import json, re, sys, time, urllib.request, urllib.parse, urllib.error
titles = """Dolomites|Pilates|Lake Como|Amalfi Coast|Santorini|Mykonos|Phuket|Bali|Machu Picchu|Patagonia|Kyoto|Reykjavík|Banff National Park|Yosemite National Park|Swiss Alps|Tuscany|Provence|Cinque Terre|Dubrovnik|Marrakesh|Yoga|Zumba|CrossFit|High-intensity interval training|Kettlebell|Burpee (exercise)|Barre (exercise)|Kombucha|Ramen|Matcha|Sourdough|Avocado toast|Smoothie|Airbnb|Instagram|TikTok|Spotify|Netflix|Podcast|Vlog|Smartphone|Wi-Fi|Bluetooth|Cryptocurrency|Taylor Swift|Beyoncé|Formula One|Tour de France|Wimbledon Championships|Marathon|Triathlon|Surfing|Snowboarding|Skiing|Hiking|Camping|Sushi|Croissant|Espresso|Prosecco|Gelato|Tapas|Paella|Pho|Burrito|Quinoa|Hummus|Meditation|Mindfulness|Ayurveda|Acupuncture|Chiropractic|Physical therapy""".split('|')
out = open(sys.argv[1], 'w', encoding='utf-8')
n = 0
for t in titles:
    url = 'https://en.wikipedia.org/w/api.php?' + urllib.parse.urlencode({'action':'query','prop':'extracts','explaintext':1,'format':'json','redirects':1,'titles':t})
    req = urllib.request.Request(url, headers={'User-Agent':'unluminous-spell-research/1.0 (https://unluminous.com)'})
    for attempt in range(6):
        try:
            data = json.load(urllib.request.urlopen(req)); break
        except urllib.error.HTTPError as e:
            if e.code != 429: raise
            time.sleep(10 * (attempt + 1))
    time.sleep(2)
    for p in data['query']['pages'].values():
        text = p.get('extract','')
        for w in re.findall(r"[^\W\d_]+(?:['’][^\W\d_]+)*", text):
            out.write(w + '\n'); n += 1
print(n)
