const fs = require('fs');

const SAVEDIR = 'saved_roles';

module.exports = {
    startup: function(client) {
        if (!fs.existsSync(`../${SAVEDIR}`)) {
            fs.mkdirSync(`../${SAVEDIR}`);
        }
        client.guilds.cache.forEach((guild) => {
            if (!fs.existsSync(`../${SAVEDIR}/${guild.id}.json`)) {
                console.log(`Missing guild save file!\nCreating new save file for guild [${guild.name} | ${guild.id}]!`);
                fs.writeFileSync(`../${SAVEDIR}/${guild.id}.json`, `{}`);
            }
            startupRoleSave(guild);
        });
    },
    updateRoles: async function(newMember) {
        let guildId = newMember.guild.id;
        let memberId = newMember.id;
        let json = JSON.parse(fs.readFileSync(`../${SAVEDIR}/${guildId}.json`));

        if (json[memberId]) {
            let roles = new Array();
            let roleNames = new Array();
            json[memberId].roles.forEach((role) => {
                roles.push(role.id);
                roleNames.push(role.name);
            });
            await newMember.roles.add(roles);
            console.log(`Granting roles: ${roleNames} to [${newMember.user.username} | ${memberId}]`);
        } else {
            console.log('There is no roles to return!');
        }
    },
    saveRoles: function(newMember) {
        let guildId = newMember.guild.id;
        console.log(`Saving roles [${newMember.guild.name} | ${guildId}]`);

        let rawJson = fs.readFileSync(`../${SAVEDIR}/${guildId}.json`);
        let json = JSON.parse(rawJson);

        if (json[newMember.id]) {
            json[newMember.id].roles = newMember.roles.cache;
        } else {
            json[newMember.id] = {
                'username': newMember.user.username,
                'roles': newMember.roles.cache
            };
        }

        fs.writeFileSync(`../${SAVEDIR}/${guildId}.json`, JSON.stringify(json));
        console.log("Wrote files");
    }
}

function startupRoleSave(guild) {
    let guildId = guild.id;
    let json = JSON.parse(fs.readFileSync(`../${SAVEDIR}/${guildId}.json`));
    console.log(`Saving roles [${guild.name} | ${guildId}]`);
    guild.members.cache.forEach((member) => {
        json[member.id] = {
            'username': member.user.username,
            'roles': member.roles.cache
        };
    });

    fs.writeFileSync(`../${SAVEDIR}/${guildId}.json`, JSON.stringify(json));
}