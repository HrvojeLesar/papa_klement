module.exports = {
    handleBan: async function(guild, user) {
        await guild.members.unban(user.id);
        let invite = await getFirstTextChannel(guild).createInvite({maxUses: 1});
        user.send(`${invite.url}`);
    }
}

function getFirstTextChannel(guild) {
    let guildChannels = guild.channels.cache;
    let guildChannelsArray = guildChannels.array();
    for (let i = 0; i < guildChannels.size; i++) {
        if (guildChannelsArray[i].type === 'text') {
            return guildChannelsArray[i];
        }
    }
    console.log("Cannot find text channel");
    return guildChannelsArray[0];
}