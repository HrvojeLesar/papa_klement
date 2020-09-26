const MATTEMOJI = '<:banajmatijosa:621685158600245248>';
const SERVER = '173766075484340234';
const MATTID = '252114544485335051';

const ALLOWEDMEMBERS = [
    // Jo
    '132286945031094272',
    // Qubacc
    '170561008786604034',
    // Fico
    '245956125713760258',
    // Znidaric
    '268420122090274816',
    // Fabac
    '344472419085582347',
    // Seba
    '344121954124431360',
    // Domi
    '302763402944839680'
];

const COOLDOWN = 3600;
let lastBan = 0;

module.exports = {
    banaj: function(message, guild) {
        console.log
        if (guild.id == SERVER) {
            if (message.content.includes(MATTEMOJI)) {
                if (ALLOWEDMEMBERS.includes(message.author.id)) {
                    let timeNow = Math.floor(Date.now() / 1000);
                    if (timeNow - lastBan > COOLDOWN) {
                        lastBan = timeNow;
                        message.channel.send("Ajde bok Matijoš!", { tts: true });
                        delayBan(message);
                    } else {
                        message.channel.send(`Necem ga jos banati! (${COOLDOWN - (timeNow - lastBan)} s)`);
                    }
                }
            }
        }
        return;
    }
}

function delayBan(message) {
    setTimeout(() => {
        try {
            let matt = message.guild.members.cache.get(MATTID);
            matt.ban();
        } catch (e) {
            console.log(e);
        }
    }, 4000);
}